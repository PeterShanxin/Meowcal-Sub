use crate::app_state::AppState;
use crate::capture;
use crate::config::{save_config, AppConfig, CaptureRegion};
use crate::engine_status::{self, EngineStatusSnapshot};
use crate::event_payloads::{CaptureStatusPayload, TranslationPayload};
use crate::ipc::{
    IpcMessage, OverlaySettingsData, RegionData, SetRegionPayload, SettingsSyncPayload,
};
use crate::llm::{
    BackendInfo, ContextCompressionScheduler, FoundryContextSummarizer, FoundryLocalBackend,
    FoundryLocalPhase, TranslationDiagnostics, TranslationManager, TranslationOutcome,
    TranslatorBackend,
};
use crate::ocr::WindowsOcr;
use crate::overlay;
use crate::overlay_ipc::send_overlay_message;
use crate::pipeline_repeat_policy as repeat_policy;
use crate::selector_window::{self, OpenAreaSelectorResult, SelectorSnapshot};
use crate::sync_utils::lock_or_recover;
use crate::system_info::SystemInfo;
use crate::wizard_contracts::WizardTranslationTest;
use crate::{hy_mt_installer, hy_mt_runtime};
use scopeguard::defer;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{async_runtime, AppHandle, Emitter, Manager, State};
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

// =============================================================================
// SYSTEM INFO
// =============================================================================

/// Get information about the system
///
/// Called from JavaScript: `const info = await invoke('get_system_info');`
#[tauri::command]
pub fn get_system_info() -> SystemInfo {
    crate::system_info::describe()
}

// =============================================================================
// OCR LANGUAGE MANAGEMENT
// =============================================================================

/// List OCR language packs installed on this system.
/// Returns BCP-47 tags (e.g. ["en-US", "zh-CN"]).
#[tauri::command]
pub async fn get_ocr_languages() -> Vec<String> {
    crate::ocr_language_packs::available().await
}

/// Install an OCR language pack via an elevated PowerShell window.
/// Triggers a UAC prompt — the user must approve the elevation.
#[tauri::command]
pub async fn install_ocr_language(language_tag: String) -> Result<(), String> {
    crate::ocr_language_packs::install(language_tag).await
}

// =============================================================================
// TIMING CONSTANTS - Translation Loop & UI
// =============================================================================
// These control timing behavior in the translation loop. Grouped here for
// visibility; tune together to balance responsiveness vs. stability.

/// Overlay fade-out duration - MUST match `OVERLAY_VISIBILITY_FADE_MS` in overlay.js
const OVERLAY_HIDE_FADE_MS: u64 = 220;

// =============================================================================
// FOUNDRY LOCAL COMMANDS
// =============================================================================

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundryLocalStatus {
    pub cli_available: bool,
    pub service_running: bool,
    pub service_url: Option<String>,
    pub models: Vec<String>,
    /// Model configured in settings (None = Auto).
    pub configured_model: Option<String>,
    /// Resolved model that will be used (if available).
    pub selected_model: Option<String>,
    pub notes: String,
    /// Granular Foundry Local phase (e.g. notInstalled, notRunning, noModels, preparing, ready).
    pub phase: FoundryLocalPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probe: Option<crate::llm::FoundryProbeSnapshot>,
}

fn foundry_status_from_snapshot(snapshot: EngineStatusSnapshot) -> FoundryLocalStatus {
    FoundryLocalStatus {
        cli_available: snapshot.cli_available,
        service_running: snapshot.service_running,
        service_url: snapshot.service_url,
        models: snapshot.models,
        configured_model: snapshot.configured_model,
        selected_model: snapshot.selected_model,
        notes: snapshot.notes,
        phase: snapshot.phase,
        probe: snapshot.probe,
    }
}

/// Get the status of Foundry Local service (fast, no probe)
#[tauri::command]
pub async fn get_foundry_local_status(
    state: State<'_, AppState>,
) -> Result<FoundryLocalStatus, String> {
    state.startup_gate.wait_until_ready().await?;
    let config = {
        let guard = lock_or_recover(&state.config);
        guard.translation.foundry_local.clone()
    };
    engine_status::get_status_tauri(config)
        .await
        .map(foundry_status_from_snapshot)
}

/// List available models from Foundry Local
#[tauri::command]
pub async fn list_foundry_local_models(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let config = {
        let guard = lock_or_recover(&state.config);
        guard.translation.foundry_local.clone()
    };
    if config.managed_runtime.is_some() {
        return Ok(config.model.clone().into_iter().collect());
    }

    let backend = FoundryLocalBackend::new(config);
    backend.refresh_service_status();
    backend.list_models().await.map_err(|e| e.to_string())
}

/// Refresh Foundry Local service status (re-detect service + fast probe)
#[tauri::command]
pub async fn refresh_foundry_local_status(
    state: State<'_, AppState>,
) -> Result<FoundryLocalStatus, String> {
    let config = {
        let guard = lock_or_recover(&state.config);
        guard.translation.foundry_local.clone()
    };
    engine_status::refresh_status_tauri(config)
        .await
        .map(foundry_status_from_snapshot)
}

/// Prepare Foundry Local (attempt to start service + slow warmup probe)
#[tauri::command]
pub async fn prepare_foundry_local(
    state: State<'_, AppState>,
) -> Result<FoundryLocalStatus, String> {
    let config = {
        let guard = lock_or_recover(&state.config);
        guard.translation.foundry_local.clone()
    };
    engine_status::prepare_tauri(config)
        .await
        .map(foundry_status_from_snapshot)
}

/// Make Foundry Local ready (start service if needed + keep probing until ready or timeout).
#[tauri::command]
pub async fn make_foundry_ready(state: State<'_, AppState>) -> Result<FoundryLocalStatus, String> {
    let config = {
        let guard = lock_or_recover(&state.config);
        guard.translation.foundry_local.clone()
    };
    engine_status::make_ready_tauri(config)
        .await
        .map(foundry_status_from_snapshot)
}
// =============================================================================
// SETTINGS COMMANDS
// =============================================================================

/// Get the current app settings
///
/// Called from JavaScript: `const settings = await invoke('get_settings');`
#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppConfig, String> {
    crate::settings_service::current(&state).await
}

/// Save new app settings
///
/// Called from JavaScript: `await invoke('save_settings', { settings: { ... } });`
#[tauri::command]
pub async fn save_settings(
    settings: AppConfig,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    crate::settings_service::save(app, &state, settings).await
}

// =============================================================================
// CAPTURE REGION COMMANDS
// =============================================================================

/// Set the screen region to capture
///
/// Called from JavaScript:
/// `await invoke('set_capture_region', { x: 100, y: 100, width: 800, height: 100, scaleFactor: 1.0 });`
#[tauri::command]
pub fn set_capture_region(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    scale_factor: f64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    info!(
        "Setting capture region: ({}, {}) {}x{}",
        x, y, width, height
    );

    crate::app_state::validate_capture_region(width, height, scale_factor)?;

    state.set_capture_region(
        CaptureRegion {
            x,
            y,
            width,
            height,
        },
        scale_factor,
    );

    Ok(())
}

/// Get the current capture region (if set)
///
/// Called from JavaScript: `const region = await invoke('get_capture_region');`
#[tauri::command]
pub fn get_capture_region(state: State<'_, AppState>) -> Option<CaptureRegion> {
    state.current_capture_region()
}

/// Open the area selector overlay window
///
/// Called from JavaScript: `await invoke('open_area_selector');`
#[tauri::command]
pub async fn open_area_selector(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<OpenAreaSelectorResult, String> {
    selector_window::open(app, &state.selector_snapshot).await
}

/// Get the most recent selector background snapshot (if available).
///
/// Called from JavaScript (selector window): `const snap = await invoke('get_selector_snapshot');`
#[tauri::command]
pub fn get_selector_snapshot(state: State<'_, AppState>) -> Option<SelectorSnapshot> {
    selector_window::snapshot(&state.selector_snapshot)
}

/// Close the area selector overlay window
///
/// Called from JavaScript: `await invoke('close_area_selector');`
#[tauri::command]
pub async fn close_area_selector(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    selector_window::close(&app, &state.selector_snapshot)
}

// =============================================================================
// TRANSLATION COMMANDS
// =============================================================================

/// Result of a capture attempt with session state
enum CaptureAttemptResult {
    /// Capture succeeded with the image data
    Success(capture::CaptureResult),
    /// Capture failed and should be retried after a delay
    RetryAfterDelay,
}

/// State for the capture session fallback logic
struct CaptureSessionState {
    use_persistent: bool,
    failure_count: u32,
    fallback_notified: bool,
}

impl CaptureSessionState {
    fn new(use_persistent: bool) -> Self {
        Self {
            use_persistent,
            failure_count: 0,
            fallback_notified: false,
        }
    }
}

/// Attempt to capture the screen region, handling session recovery and fallback.
///
/// This flattens the deeply nested capture logic into a single function.
fn try_capture(
    region: &CaptureRegion,
    state: &mut CaptureSessionState,
    app: &AppHandle,
) -> CaptureAttemptResult {
    // Try persistent session first if available
    if state.use_persistent {
        match capture::capture_with_session(region) {
            Ok(result) => {
                state.failure_count = 0;
                return CaptureAttemptResult::Success(result);
            }
            Err(e) => {
                state.failure_count += 1;
                warn!(
                    "⚠️ Session capture failed (attempt {}): {}",
                    state.failure_count, e
                );

                // Try to recover the session on first failure
                if state.failure_count == 1 {
                    capture::close_capture_session();
                    if let Err(restart_err) = capture::init_capture_session() {
                        warn!(
                            "⚠️ Failed to restart capture session, falling back: {}",
                            restart_err
                        );
                        state.use_persistent = false;
                    } else {
                        info!("✅ Capture session restarted");
                    }
                } else if state.failure_count >= 3 {
                    warn!("⚠️ Disabling persistent capture after repeated failures");
                    capture::close_capture_session();
                    state.use_persistent = false;
                }
            }
        }
    }

    // Fall back to smart_capture
    match capture::smart_capture(region) {
        Ok((result, using_fallback)) => {
            if using_fallback && !state.fallback_notified {
                state.fallback_notified = true;
                let status = CaptureStatusPayload {
                    using_fallback: true,
                    message: "Using GDI fallback - video content may not capture correctly"
                        .to_string(),
                    is_error: false,
                };
                let _ = app.emit("capture-status", status);
            }
            CaptureAttemptResult::Success(result)
        }
        Err(e) => {
            warn!("⚠️ Capture failed: {}", e);
            let status = CaptureStatusPayload {
                using_fallback: false,
                message: format!("Capture failed: {}", e),
                is_error: true,
            };
            let _ = app.emit("capture-status", status);
            CaptureAttemptResult::RetryAfterDelay
        }
    }
}

/// Check if translation is currently running
///
/// Called from JavaScript: `const running = await invoke('is_translation_running');`
/// This allows the frontend to sync button state with backend on page load/reload.
#[tauri::command]
pub fn is_translation_running(state: State<'_, AppState>) -> bool {
    let is_running = lock_or_recover(&state.is_running);
    *is_running
}

/// List translation backends and their readiness
#[tauri::command]
pub async fn list_translation_backends(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<BackendInfo>, String> {
    let config = {
        let guard = lock_or_recover(&state.config);
        guard.translation.clone()
    };

    let diagnostics = state.translation_diagnostics.clone();
    let app_handle = app.clone();

    async_runtime::spawn_blocking(move || {
        let manager = TranslationManager::new(config, app_handle, diagnostics);
        manager.list_backends()
    })
    .await
    .map_err(|err| {
        let message = format!("Translation backend listing task failed: {}", err);
        warn!("{}", message);
        message
    })
}

/// Translate a single text input (for debugging/UI testing)
#[tauri::command]
pub async fn translate_once(
    text: String,
    source_language: String,
    target_language: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<TranslationOutcome, String> {
    let config = {
        let guard = lock_or_recover(&state.config);
        guard.translation.clone()
    };

    let diagnostics = state.translation_diagnostics.clone();
    let manager = TranslationManager::new(config, app, diagnostics);
    Ok(manager
        .translate_with_fallback(&text, &source_language, &target_language)
        .await)
}

/// Get diagnostics for translation backends
#[tauri::command]
pub async fn get_translation_diagnostics(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<TranslationDiagnostics, String> {
    let config = {
        let guard = lock_or_recover(&state.config);
        guard.translation.clone()
    };

    let diagnostics = state.translation_diagnostics.clone();
    let app_handle = app.clone();

    async_runtime::spawn_blocking(move || {
        let manager = TranslationManager::new(config, app_handle, diagnostics);
        manager.diagnostics_snapshot()
    })
    .await
    .map_err(|err| {
        let message = format!("Translation diagnostics task failed: {}", err);
        warn!("{}", message);
        message
    })
}

#[tauri::command]
pub async fn start_translation(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    info!(">>> START_TRANSLATION COMMAND CALLED <<<");
    info!("Starting translation...");

    let region = {
        let region_guard = lock_or_recover(&state.capture_region);
        match *region_guard {
            Some(r) => r,
            None => return Err("No capture region set. Please select an area first.".to_string()),
        }
    };

    {
        let mut is_running = lock_or_recover(&state.is_running);
        if *is_running {
            return Err("Translation is already running".to_string());
        }
        *is_running = true;
    }
    let pipeline_clock = Arc::clone(&state.pipeline_clock);
    let session_id = pipeline_clock.begin_session();

    let scale_factor = {
        let scale_guard = lock_or_recover(&state.capture_scale_factor);
        *scale_guard
    };
    let capture_region = region.scaled(scale_factor);
    debug!(
        "Capture scale factor: {}, logical: {:?}, physical: {:?}",
        scale_factor, region, capture_region
    );

    let (interval_ms, source_language, target_language, translation_config) = {
        let config = lock_or_recover(&state.config);
        (
            config.capture_interval_ms,
            config.source_language.clone(),
            config.target_language.clone(),
            config.translation.clone(),
        )
    };

    let recognition_mode = crate::ocr::RecognitionMode {
        multi_pass: translation_config.ocr.enable_multi_pass,
        multi_pass_count: translation_config.ocr.multi_pass_count,
        preprocessing: translation_config.ocr.preprocessing_enabled,
        grayscale: translation_config.ocr.grayscale,
        contrast_enhancement: translation_config.ocr.contrast_enhancement,
        binarize: translation_config.ocr.binarize,
    };
    let ocr_validation_strictness = translation_config.ocr.validation_strictness;

    let min_significant_chars = ocr_validation_strictness.min_significant_chars();

    debug!(?translation_config.ocr, min_significant_chars, "OCR settings");

    // Pace to a deadline: a frame that ran the translator must not also pay a
    // full interval before the next capture.
    let pacer = crate::pipeline_pacing::Pacer::new(interval_ms);

    let context_enabled = translation_config.enable_context_aware;
    let translation_config_for_summary = translation_config.clone();

    let (stop_tx, mut stop_rx) = watch::channel(false);

    {
        let mut stop_signal = lock_or_recover(&state.stop_signal);
        *stop_signal = Some(stop_tx);
    }

    info!(
        "✅ Translation started! Interval: {}ms, Target: {}",
        interval_ms, target_language
    );

    if let Err(e) = overlay::show_overlay(&app).await {
        warn!("⚠️ Failed to show overlay: {}", e);
    }
    if let Err(e) = overlay::update_overlay_region(&app, &region) {
        warn!("⚠️ Failed to update overlay region: {}", e);
    }

    send_overlay_message(&app, IpcMessage::new("Overlay.Show")).await;

    let payload = SetRegionPayload {
        region: RegionData::from(&region),
    };
    send_overlay_message(&app, IpcMessage::with_payload("Overlay.SetRegion", payload)).await;

    let settings_payload = {
        let config = lock_or_recover(&state.config);
        SettingsSyncPayload {
            overlay: OverlaySettingsData::from(&config.overlay),
        }
    };
    send_overlay_message(
        &app,
        IpcMessage::with_payload("Settings.Sync", settings_payload),
    )
    .await;

    let diagnostics = state.translation_diagnostics.clone();
    let translation_manager = Arc::new(TranslationManager::new(
        translation_config,
        app.clone(),
        diagnostics,
    ));
    let context_generation = Arc::new(AtomicU64::new(0));
    let context_compression = Arc::new(ContextCompressionScheduler::new(
        Arc::clone(&translation_manager),
        Arc::new(FoundryContextSummarizer::new(
            translation_config_for_summary.clone(),
        )),
        Arc::clone(&context_generation),
        translation_config_for_summary.context_summary_cooldown_ms as u64,
    ));

    let app_for_region = app.clone();

    let translation_handle = tokio::spawn(async move {
        let app_for_guard = app.clone();
        defer! {
            let state = app_for_guard.state::<AppState>();
            *lock_or_recover(&state.is_running) = false;
            capture::close_capture_session();
            info!("Translation loop cleanup complete");
        }

        info!("Translation loop started");
        info!("Initializing OCR engine (language={})", source_language);

        #[cfg(target_os = "windows")]
        {
            use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
            if let Err(e) = unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
                warn!("⚠️ Failed to initialize WinRT on worker thread: {}", e);
            }
        }

        let ocr = match WindowsOcr::with_language(&source_language) {
            Ok(o) => {
                info!("OCR initialized with language: {}", source_language);
                o.for_capture_scale(scale_factor)
            }
            Err(e) => {
                warn!(
                    "⚠️ OCR language '{}' not available: {}. Falling back to user profile languages.",
                    source_language, e
                );
                let _ = app.emit(
                    "capture-status",
                    CaptureStatusPayload {
                        using_fallback: true,
                        message: format!(
                            "OCR language '{}' is not installed. Using system default. \
                             Install it: Windows Settings > Time & Language > Language & Region.",
                            source_language
                        ),
                        is_error: false,
                    },
                );
                match WindowsOcr::new() {
                    Ok(o) => o.for_capture_scale(scale_factor),
                    Err(e) => {
                        warn!("❌ Failed to initialize OCR: {}", e);
                        return;
                    }
                }
            }
        };

        let translator = crate::pipeline_translation::Translator::new(
            app.clone(),
            Arc::clone(&translation_manager),
            Arc::clone(&pipeline_clock),
            session_id,
            source_language.clone(),
            target_language.clone(),
        );

        // Several lines rather than one: see `ocr_recent_lines` and issue #59.
        let mut recent_lines = crate::ocr_recent_lines::RecentLines::new();
        let mut last_attempt_at = Instant::now()
            .checked_sub(crate::pipeline_repeat_policy::MOCK_RETRY_COOLDOWN)
            .unwrap_or_else(Instant::now);
        let mut last_capture_region: Option<CaptureRegion> = None;
        let mut notices = crate::pipeline_notices::Notices::new();

        let use_persistent = match capture::init_capture_session() {
            Ok(_) => {
                info!("✅ Persistent capture session initialized");
                true
            }
            Err(e) => {
                warn!(
                    "⚠️ Failed to init persistent session, will use per-frame capture: {}",
                    e
                );
                false
            }
        };
        let mut capture_state = CaptureSessionState::new(use_persistent);
        // A region taller than one subtitle also holds scene text, credits and
        // static overlays. See `ocr::BandFilter`.
        let mut band_filter = crate::ocr::BandFilter::new(interval_ms.into());

        loop {
            if *stop_rx.borrow() {
                info!("🛑 Stop signal received, exiting loop");
                break;
            }

            if stop_rx.has_changed().unwrap_or(false) {
                let _ = stop_rx.borrow_and_update();
                if *stop_rx.borrow() {
                    info!("🛑 Stop signal received, exiting loop");
                    break;
                }
            }

            let current_capture_region = {
                let state = app_for_region.state::<AppState>();
                let region_opt = *lock_or_recover(&state.capture_region);
                let scale = *lock_or_recover(&state.capture_scale_factor);
                match region_opt {
                    Some(r) => {
                        let scaled = r.scaled(scale);
                        let (screen_width, screen_height) = capture::get_screen_dimensions();

                        if scaled.intersects_origin_bounds(screen_width, screen_height) {
                            scaled
                                .clamp_to_bounds(screen_width, screen_height)
                                .unwrap_or(scaled)
                        } else {
                            scaled
                        }
                    }
                    None => {
                        warn!("⚠️ No capture region set, skipping frame");
                        tokio::time::sleep(pacer.period()).await;
                        continue;
                    }
                }
            };

            if last_capture_region
                .map(|prev| prev != current_capture_region)
                .unwrap_or(false)
            {
                debug!("Capture region changed, resetting subtitle context");
                translation_manager.reset_context();
                recent_lines.clear();
                translator.set_last_backend_was_mock(true);
                last_attempt_at = Instant::now()
                    .checked_sub(crate::pipeline_repeat_policy::MOCK_RETRY_COOLDOWN)
                    .unwrap_or_else(Instant::now);
                // Keep this monotonic: resetting to 0 can allow old summarization tasks to
                // accidentally validate again once the counter reaches the same value.
                context_generation.fetch_add(1, Ordering::SeqCst);
            }
            last_capture_region = Some(current_capture_region);

            debug!("📸 Capturing region: {:?}", current_capture_region);
            let frame_started = Instant::now();
            let token = pipeline_clock.next_capture(session_id);

            let capture_started = Instant::now();
            let capture_result =
                match try_capture(&current_capture_region, &mut capture_state, &app) {
                    CaptureAttemptResult::Success(result) => result,
                    CaptureAttemptResult::RetryAfterDelay => {
                        tokio::time::sleep(pacer.remaining_for(frame_started)).await;
                        continue;
                    }
                };
            let capture_ms = capture_started.elapsed().as_millis() as u64;

            if *stop_rx.borrow() {
                info!("🛑 Stop signal received, aborting current frame");
                break;
            }
            if !pipeline_clock.is_current(token) {
                info!(
                    session_id,
                    capture_id = token.capture_id,
                    "Stale capture, before OCR"
                );
                continue;
            }

            let ocr_started = Instant::now();
            let frame_data = &capture_result.data;
            let (frame_width, frame_height) = (capture_result.width, capture_result.height);
            let ocr_result = match recognition_mode
                .recognize(&ocr, frame_data, frame_width, frame_height)
                .await
            {
                Ok(result) => result,
                Err((error, what)) => {
                    warn!("⚠️ {}: {}", what, error);
                    tokio::time::sleep(pacer.remaining_for(frame_started)).await;
                    continue;
                }
            };
            let ocr_ms = ocr_started.elapsed().as_millis() as u64;

            if *stop_rx.borrow() || !pipeline_clock.is_current(token) {
                info!(
                    session_id,
                    capture_id = token.capture_id,
                    "Stale capture, after OCR"
                );
                if *stop_rx.borrow() {
                    break;
                }
                continue;
            }

            let ocr_result = band_filter.apply(ocr_result);

            if ocr_result.is_empty() {
                debug!("[FILTER: {}] skipping", band_filter.skip_reason());
                let held = band_filter.held_lines();
                let busy = translator.is_busy();
                if let Some(quiet) = notices.quiet_region(session_id, token.capture_id, held, busy)
                {
                    // The cleared line has to be translatable again: without this
                    // the duplicate filter would suppress the identical subtitle
                    // when it returns, leaving the overlay permanently blank.
                    recent_lines.clear();
                    let _ = app.emit("translation-update", quiet);
                }

                tokio::time::sleep(pacer.remaining_for(frame_started)).await;
                continue;
            }

            notices.saw_text();

            let current_text = ocr_result.text.trim().to_string();

            if let Some(rejection) = crate::ocr_gate::classify(&current_text, min_significant_chars)
            {
                debug!(
                    "[FILTER: {}] OCR text ({} chars, minimum {})",
                    rejection.as_str(),
                    current_text.chars().count(),
                    min_significant_chars
                );
                let busy = translator.is_busy();
                if let Some(notice) =
                    notices.unreadable_source(session_id, token.capture_id, rejection, busy)
                {
                    let _ = app.emit("translation-update", notice);
                }
                tokio::time::sleep(pacer.remaining_for(frame_started)).await;
                continue;
            }

            let now = Instant::now();
            let line_change = recent_lines.classify(&current_text, now);
            let mut force_retry_duplicate = false;
            if line_change == crate::ocr_stability::LineChange::Repeat {
                match repeat_policy::decide(
                    translator.last_backend_was_mock(),
                    now.duration_since(last_attempt_at),
                ) {
                    repeat_policy::RepeatAction::Skip(reason) => {
                        debug!(source = %current_text, "[FILTER: {reason}] OCR text");
                        translation_manager.record_ocr_line(&current_text);
                        tokio::time::sleep(pacer.remaining_for(frame_started)).await;
                        continue;
                    }
                    repeat_policy::RepeatAction::RetryPassthrough => force_retry_duplicate = true,
                }
            }

            if !force_retry_duplicate
                && context_enabled
                && translation_manager.is_duplicate(&current_text)
            {
                debug!("[FILTER: duplicate_context] OCR text");
                translation_manager.record_ocr_line(&current_text);
                tokio::time::sleep(pacer.remaining_for(frame_started)).await;
                continue;
            }
            // Deliberately no "is this read worse than the last?" check here: a
            // noisier re-read is `Repeat` and was skipped above, so only `New`
            // and `Extended` remain. `Extended` *contains* the last read - a
            // garbled prefix is refused in `ocr_stability`. See `ocr_corruption`.

            // Claiming the single translation slot is the point of no return:
            // everything below records the line as handled, so it runs only if
            // the claim succeeded. See `Translator` for why it is one at a time
            // and why declining is safe.
            let context_prompt = translation_manager.get_context_prompt();
            let taken = translator.try_spawn(
                crate::pipeline_translation::Frame {
                    token,
                    text: current_text.clone(),
                    context_prompt,
                    started: frame_started,
                    capture_ms,
                    ocr_ms,
                },
                stop_rx.clone(),
            );
            if !taken {
                debug!(source = %current_text, "[DEFER: translating] OCR text");
                tokio::time::sleep(pacer.remaining_for(frame_started)).await;
                continue;
            }

            recent_lines.remember(&current_text, now);
            notices.translated();
            last_attempt_at = now;
            // Bumped here, not in the task: see `Translator`.
            context_generation.fetch_add(1, Ordering::SeqCst);
            info!("📝 OCR detected ({} chars)", current_text.chars().count());
            translation_manager.record_ocr_line(&current_text);

            // Check if context needs compression (async, don't block).
            // Scheduling, cooldown, stability delay, retries, and
            // restore/cap semantics live in `ContextCompressionScheduler`.
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            context_compression.schedule_if_needed(now_ms, stop_rx.clone());

            tokio::time::sleep(pacer.remaining_for(frame_started)).await;
        }

        if pipeline_clock.is_session_current(session_id) {
            let stopped_token = pipeline_clock.next_capture(session_id);
            let _ = app.emit(
                "translation-update",
                TranslationPayload::stopped(session_id, stopped_token.capture_id),
            );
        }

        info!("Translation loop ended");
    });

    // Monitor the translation task for panics - log but don't propagate
    tokio::spawn(async move {
        match translation_handle.await {
            Ok(()) => {
                // Task completed normally
            }
            Err(join_error) => {
                if join_error.is_panic() {
                    // Extract panic message if possible
                    let panic_info = join_error.into_panic();
                    let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = panic_info.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "unknown panic".to_string()
                    };
                    error!(
                        "❌ Translation loop panicked: {}. Cleanup was handled by scopeguard.",
                        panic_msg
                    );
                } else if join_error.is_cancelled() {
                    info!("Translation loop was cancelled");
                }
            }
        }
    });

    Ok(())
}

/// Stop the translation process
///
/// Called from JavaScript: `await invoke('stop_translation');`
#[tauri::command]
pub async fn stop_translation(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    info!("Stopping translation...");
    let stopped_session_id = state.pipeline_clock.invalidate_session();

    // Send the stop signal
    {
        let stop_signal = lock_or_recover(&state.stop_signal);
        if let Some(ref sender) = *stop_signal {
            let _ = sender.send(true);
            info!("Stop signal sent");
        }
    }

    // Clear the stop signal sender
    {
        let mut stop_signal = lock_or_recover(&state.stop_signal);
        *stop_signal = None;
    }

    // Close the capture session
    capture::close_capture_session();

    // Send hide message to WinUI3 OverlayHost
    send_overlay_message(&app, IpcMessage::new("Overlay.Hide")).await;

    // Fade out legacy WebView overlay before hiding the window (premium UX).
    //
    // We emit the visibility event first so the frontend can animate while the window
    // is still visible. After a short delay, we hide the window for real.
    let _ = app.emit("overlay-visibility", false);
    tokio::time::sleep(Duration::from_millis(OVERLAY_HIDE_FADE_MS)).await;
    if let Some(window) = app.get_webview_window("overlay") {
        if let Err(e) = window.hide() {
            warn!("⚠️ Failed to hide legacy overlay window: {}", e);
        }
    } else {
        warn!("⚠️ Overlay window not found");
    }
    let _ = app.emit(
        "translation-update",
        TranslationPayload::stopped(stopped_session_id, 0),
    );

    info!("✅ Translation stopped!");
    Ok(())
}

// =============================================================================
// OVERLAY COMMANDS
// =============================================================================

// =============================================================================
// FOUNDRY SETUP WIZARD COMMANDS
// =============================================================================

/// Show the foundry-wizard window, resetting state for a fresh run
#[tauri::command]
pub fn open_foundry_wizard(app: AppHandle) -> Result<(), String> {
    crate::wizard_window::open(&app)
}

/// Hide the foundry-wizard window and notify the main window
#[tauri::command]
pub fn close_foundry_wizard(
    app: AppHandle,
    model_downloaded: bool,
    selected_model: Option<String>,
) -> Result<(), String> {
    crate::wizard_window::close(&app, model_downloaded, selected_model)
}

#[tauri::command]
pub async fn wizard_install_engine(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let manifest =
        crate::engine_manifest::EngineManifest::shipped().map_err(|error| error.to_string())?;
    // Falls back to the independently recorded root, so a lost registration no
    // longer sends setup to a different directory and a re-download (#65).
    let cache_dir = {
        let config = lock_or_recover(&state.config);
        crate::engine_recovery::install_cache_root(&config.translation.foundry_local)
    };

    match hy_mt_installer::install(&app, cache_dir).await {
        Ok(paths) => {
            let runtime = paths.managed_config(&manifest);
            let updated = {
                let mut config = lock_or_recover(&state.config);
                config.translation.enable_foundry_local = true;
                config.translation.allow_mock_fallback = false;
                config.translation.foundry_local.model = Some(manifest.model.id.clone());
                config.translation.foundry_local.endpoint_url =
                    Some(hy_mt_runtime::endpoint_url(&runtime));
                config.translation.foundry_local.managed_runtime = Some(runtime);
                // Kept independently of the runtime record so a future config
                // problem cannot hide this install from setup (#65).
                config.translation.foundry_local.engine_cache_root =
                    crate::engine_recovery::cache_root_of(&paths);
                config.clone()
            };
            save_config(&app, &updated)?;
            let _ = app.emit_to(
                "foundry-wizard",
                "wizard-download-complete",
                serde_json::json!({"success": true}),
            );
        }
        Err(error) => {
            warn!("Local Translation Engine install failed: {}", error);
            let _ = app.emit_to(
                "foundry-wizard",
                "wizard-download-complete",
                serde_json::json!({"success": false, "error": error}),
            );
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn wizard_start_service(state: State<'_, AppState>) -> Result<String, String> {
    let runtime = {
        let config = lock_or_recover(&state.config);
        config
            .translation
            .foundry_local
            .managed_runtime
            .clone()
            .ok_or_else(|| "ENGINE_NOT_INSTALLED".to_string())?
    };
    hy_mt_runtime::ensure_ready(&runtime, Duration::from_secs(90)).await
}

#[tauri::command]
pub async fn wizard_test_translation(
    state: State<'_, AppState>,
    source_text: String,
    source_language: String,
    target_language: String,
) -> Result<WizardTranslationTest, String> {
    let config = {
        let config = lock_or_recover(&state.config);
        config.translation.foundry_local.clone()
    };
    let backend = FoundryLocalBackend::new(config);
    let started = Instant::now();
    let translated_text = backend
        .translate(&source_text, &source_language, &target_language)
        .await
        .map_err(|error| error.to_string())?;
    if translated_text.trim().is_empty() || translated_text.trim() == source_text.trim() {
        return Err("ENGINE_SAMPLE_TRANSLATION_FAILED".to_string());
    }
    Ok(WizardTranslationTest {
        translated_text,
        latency_ms: started.elapsed().as_millis() as u64,
    })
}
