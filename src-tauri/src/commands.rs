use crate::capture;
use crate::config::{save_config, AppConfig, CaptureRegion};
use crate::event_payloads::{CaptureStatusPayload, TranslationPayload, EMPTY_OCR_CLEAR_FRAMES};
use crate::ipc::{
    IpcMessage, IpcServer, OverlaySettingsData, RegionData, SetRegionPayload, SettingsSyncPayload,
};
use crate::llm::{
    BackendInfo, FoundryLocalBackend, FoundryLocalPhase, TranslationDiagnostics,
    TranslationDiagnosticsState, TranslationManager, TranslationOutcome, TranslatorBackend,
};
use crate::ocr::WindowsOcr;
use crate::overlay;
use crate::pipeline_session::PipelineClock;
use crate::startup_gate::StartupGate;
use crate::sync_utils::lock_or_recover;
use crate::wizard_contracts::WizardTranslationTest;
use crate::{hy_mt_installer, hy_mt_runtime};
use scopeguard::defer;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{async_runtime, AppHandle, Emitter, Manager, State};
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

// =============================================================================
// IPC HELPER
// =============================================================================

fn env_truthy(name: &str) -> bool {
    std::env::var(name)
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// Send a message to OverlayHost via IPC
pub(crate) async fn send_overlay_message(app: &AppHandle, message: IpcMessage) {
    // Premium legacy is the default. The WinUI overlay is still experimental and can be
    // opaque/black on some systems, so keep it opt-in for now.
    if !env_truthy("MEOWCAL_USE_WINUI_OVERLAY") {
        return;
    }

    if let Some(ipc_server) = app.try_state::<Arc<IpcServer>>() {
        ipc_server.send(message).await;
    } else {
        warn!("⚠️ IPC server not initialized, cannot send message");
    }
}

// Shared application state that persists across Tauri commands.

/// The application state, managed by Tauri
pub struct AppState {
    pub startup_gate: StartupGate,
    /// Current app configuration (settings)
    pub config: Mutex<AppConfig>,
    /// Whether translation is currently active
    pub is_running: Mutex<bool>,
    /// The current capture region (if set)
    pub capture_region: Mutex<Option<CaptureRegion>>,
    /// DPI scale factor for the capture region (logical -> physical)
    pub capture_scale_factor: Mutex<f64>,
    /// Stop signal sender for the translation loop
    /// When we send `true` through this, the loop stops
    pub stop_signal: Mutex<Option<watch::Sender<bool>>>,
    /// Diagnostics for translation backends
    pub translation_diagnostics: Arc<Mutex<TranslationDiagnosticsState>>,
    /// Monotonic session/capture identities used to suppress stale async results.
    pub pipeline_clock: Arc<PipelineClock>,

    /// Latest "desktop snapshot" for the area selector window.
    ///
    /// Why we need this:
    /// - On some Windows/WebView2 versions, transparent webviews regress to opaque grey/black.
    /// - The selector window is supposed to be fullscreen transparent so the user can see the desktop.
    /// - As a fallback, we capture a screenshot *before* showing the selector and render it as an image.
    pub selector_snapshot: Mutex<Option<SelectorSnapshot>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            startup_gate: StartupGate::default(),
            config: Mutex::new(AppConfig::default()),
            is_running: Mutex::new(false),
            capture_region: Mutex::new(None),
            capture_scale_factor: Mutex::new(1.0),
            stop_signal: Mutex::new(None),
            translation_diagnostics: Arc::new(Mutex::new(TranslationDiagnosticsState::default())),
            pipeline_clock: Arc::new(PipelineClock::default()),
            selector_snapshot: Mutex::new(None),
        }
    }
}

// =============================================================================
// SYSTEM INFO
// =============================================================================

/// Information about the system, returned to the UI
#[derive(Serialize)]
pub struct SystemInfo {
    /// Operating system info
    pub os: String,
    /// CPU architecture (should be aarch64 on Copilot+ PCs)
    pub arch: String,
    /// Whether we're on a Copilot+ PC (NPU available)
    pub is_copilot_plus: bool,
    /// Whether Phi Silica API is available
    pub phi_silica_available: bool,
    /// Whether Windows OCR is available
    pub windows_ocr_available: bool,
}

/// Get information about the system
///
/// Called from JavaScript: `const info = await invoke('get_system_info');`
#[tauri::command]
pub fn get_system_info() -> SystemInfo {
    info!("Getting system info...");

    // Check what features are available
    let is_arm64 = cfg!(target_arch = "aarch64");

    // TODO: Actually detect NPU presence
    // For now, assume ARM64 Windows = Copilot+ PC
    let is_copilot_plus = is_arm64 && cfg!(target_os = "windows");

    // TODO: Check if Phi Silica is available (Windows AI APIs)
    // This will be implemented when we add LLM support
    let phi_silica_available = false;

    // Windows OCR should be available on all Windows 10/11 systems
    let windows_ocr_available = cfg!(target_os = "windows");

    let info = SystemInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        is_copilot_plus,
        phi_silica_available,
        windows_ocr_available,
    };

    info!(
        "System: {} {}, Copilot+: {}, Phi Silica: {}, OCR: {}",
        info.os,
        info.arch,
        info.is_copilot_plus,
        info.phi_silica_available,
        info.windows_ocr_available
    );

    info
}

// =============================================================================
// OCR LANGUAGE MANAGEMENT
// =============================================================================

/// List OCR language packs installed on this system.
/// Returns BCP-47 tags (e.g. ["en-US", "zh-CN"]).
#[tauri::command]
pub async fn get_ocr_languages() -> Vec<String> {
    info!("Getting available OCR languages...");
    let result = async_runtime::spawn_blocking(WindowsOcr::available_languages).await;
    match result {
        Ok(Ok(langs)) => {
            info!("Found {} OCR language(s): {:?}", langs.len(), langs);
            langs
        }
        Ok(Err(e)) => {
            warn!("Failed to enumerate OCR languages: {}", e);
            Vec::new()
        }
        Err(e) => {
            warn!("OCR language enumeration task failed: {}", e);
            Vec::new()
        }
    }
}

/// Install an OCR language pack via an elevated PowerShell window.
/// Triggers a UAC prompt — the user must approve the elevation.
#[tauri::command]
pub async fn install_ocr_language(language_tag: String) -> Result<(), String> {
    // Strict allowlist: only accept known BCP-47 tags to prevent command injection
    // in the elevated PowerShell context.
    let capability_tag = match language_tag.as_str() {
        "en-US" => "en-US",
        "zh-TW" => "zh-Hant",
        "zh-CN" => "zh-Hans",
        "ja-JP" => "ja",
        "ko-KR" => "ko",
        "es-ES" => "es",
        "fr-FR" => "fr",
        "de-DE" => "de",
        _ => {
            return Err(format!(
                "Unsupported language tag: '{}'. Only known languages can be installed.",
                language_tag
            ));
        }
    }
    .to_string();

    info!(
        "Installing OCR language pack: {} (capability tag: {})",
        language_tag, capability_tag
    );

    async_runtime::spawn_blocking(move || {
        // Build the inner (elevated) PowerShell script
        let inner_script = format!(
            "Write-Host 'Installing OCR language pack: {tag}...' -ForegroundColor Cyan; \
             Write-Host ''; \
             $cap = Get-WindowsCapability -Online | Where-Object {{ $_.Name -Like 'Language.OCR*{tag}*' -and $_.State -ne 'Installed' }}; \
             if ($cap) {{ \
                 $cap | Add-WindowsCapability -Online; \
                 Write-Host ''; \
                 Write-Host 'Done! OCR language pack installed successfully.' -ForegroundColor Green \
             }} else {{ \
                 Write-Host 'Language pack is already installed or not available.' -ForegroundColor Yellow \
             }}; \
             Start-Sleep -Seconds 5",
            tag = capability_tag
        );

        // Outer PowerShell spawns an elevated inner shell via Start-Process -Verb RunAs
        let mut cmd = std::process::Command::new("powershell");
        cmd.args([
            "-NoProfile",
            "-Command",
            &format!(
                "Start-Process powershell -Verb RunAs -Wait -ArgumentList '-NoProfile -Command {}'",
                // Escape single quotes for the nested argument
                inner_script.replace('\'', "''")
            ),
        ]);

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        match cmd.status() {
            Ok(status) if status.success() => {
                info!("OCR language pack install completed for: {}", language_tag);
                Ok(())
            }
            Ok(status) => {
                let msg = format!(
                    "OCR language pack install exited with code: {:?}",
                    status.code()
                );
                warn!("{}", msg);
                // Still return Ok — the user may have cancelled the UAC prompt,
                // and we'll re-check available languages on the frontend
                Ok(())
            }
            Err(e) => Err(format!("Failed to launch installer: {}", e)),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

// =============================================================================
// TIMING CONSTANTS - Translation Loop & UI
// =============================================================================
// These control timing behavior in the translation loop. Grouped here for
// visibility; tune together to balance responsiveness vs. stability.

/// Maximum retries when summarizing context fails (transient errors)
const CONTEXT_SUMMARY_MAX_RETRIES: usize = 3;
/// Delay between context summarization retries
const CONTEXT_SUMMARY_RETRY_DELAY_MS: u64 = 500;
/// Wait time after Foundry becomes ready before summarizing (prevents race conditions)
const CONTEXT_SUMMARY_STABILITY_DELAY_MS: u64 = 900;
/// Cooldown before retrying mock backend after failures
const MOCK_RETRY_COOLDOWN_MS: u64 = 2500;
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
    if let Some(status) = managed_hy_mt_status(&config, false).await {
        return Ok(status);
    }

    async_runtime::spawn_blocking(move || build_foundry_local_status_no_probe(config))
        .await
        .map_err(|err| {
            let message = format!("Foundry Local status task failed: {}", err);
            warn!("{}", message);
            message
        })
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
    use crate::llm::FAST_PROBE_TIMEOUT_MS;

    let config = {
        let guard = lock_or_recover(&state.config);
        guard.translation.foundry_local.clone()
    };
    if let Some(status) = managed_hy_mt_status(&config, false).await {
        return Ok(status);
    }
    let configured_model = config.model.clone();

    // Build basic status synchronously
    let (backend, cli_available, service_url, service_running, models, notes) =
        async_runtime::spawn_blocking({
            let config = config.clone();
            move || {
                let backend = FoundryLocalBackend::new(config);
                backend.refresh_service_status();
                let cli_available = FoundryLocalBackend::is_cli_available();
                let service_url = FoundryLocalBackend::get_service_url_from_cli();
                let service_running = service_url.is_some();
                let models = if service_running {
                    FoundryLocalBackend::get_cached_models_from_cli()
                } else {
                    Vec::new()
                };
                let notes = backend.notes();
                (
                    backend,
                    cli_available,
                    service_url,
                    service_running,
                    models,
                    notes,
                )
            }
        })
        .await
        .map_err(|err| format!("Foundry Local status task failed: {}", err))?;

    // If service running with models, perform fast probe
    let phase = if service_running && !models.is_empty() {
        // Check probe cache first
        if backend.is_probe_cache_valid() {
            debug!("Foundry Local probe cache valid, returning ready");
            FoundryLocalPhase::Ready
        } else {
            // Run fast probe
            debug!(
                "Running fast Foundry Local probe ({}ms timeout)",
                FAST_PROBE_TIMEOUT_MS
            );
            match backend.probe_chat_completions(FAST_PROBE_TIMEOUT_MS).await {
                Ok(true) => {
                    info!("Foundry Local fast probe succeeded");
                    FoundryLocalPhase::Ready
                }
                Ok(false) => {
                    info!("Foundry Local fast probe timed out (model preparing)");
                    FoundryLocalPhase::Preparing
                }
                Err(e) => {
                    warn!("Foundry Local fast probe failed: {}", e);
                    FoundryLocalPhase::Error
                }
            }
        }
    } else {
        // Determine phase without probe
        backend.phase()
    };

    Ok(FoundryLocalStatus {
        cli_available,
        service_running,
        service_url,
        models,
        configured_model,
        selected_model: backend.selected_model(),
        notes,
        phase,
        probe: backend.probe_snapshot(),
    })
}

/// Prepare Foundry Local (attempt to start service + slow warmup probe)
#[tauri::command]
pub async fn prepare_foundry_local(
    state: State<'_, AppState>,
) -> Result<FoundryLocalStatus, String> {
    use crate::llm::SLOW_PROBE_TIMEOUT_MS;

    let config = {
        let guard = lock_or_recover(&state.config);
        guard.translation.foundry_local.clone()
    };
    if let Some(status) = managed_hy_mt_status(&config, true).await {
        return Ok(status);
    }
    let configured_model = config.model.clone();

    // Build basic status synchronously (this will attempt to start service if needed)
    let (backend, cli_available, service_url, service_running, models, mut notes) =
        async_runtime::spawn_blocking({
            let config = config.clone();
            move || {
                let backend = FoundryLocalBackend::new(config);
                backend.ensure_service_running();
                let cli_available = FoundryLocalBackend::is_cli_available();
                let service_url = FoundryLocalBackend::get_service_url_from_cli();
                let service_running = service_url.is_some();
                let models = if service_running {
                    FoundryLocalBackend::get_cached_models_from_cli()
                } else {
                    Vec::new()
                };
                let notes = backend.notes();
                (
                    backend,
                    cli_available,
                    service_url,
                    service_running,
                    models,
                    notes,
                )
            }
        })
        .await
        .map_err(|err| format!("Foundry Local prepare task failed: {}", err))?;

    // If service running with models, perform slow warmup probe
    let phase = if service_running && !models.is_empty() {
        info!(
            "Starting Foundry Local warmup probe ({}ms timeout)",
            SLOW_PROBE_TIMEOUT_MS
        );
        match backend.probe_chat_completions(SLOW_PROBE_TIMEOUT_MS).await {
            Ok(true) => {
                info!("Foundry Local warmup probe succeeded");
                notes = format!("{} Warmup complete.", notes);
                FoundryLocalPhase::Ready
            }
            Ok(false) => {
                info!("Foundry Local warmup probe timed out (model still warming up)");
                notes = format!("{} Model still warming up.", notes);
                FoundryLocalPhase::Preparing
            }
            Err(e) => {
                warn!("Foundry Local warmup probe failed: {}", e);
                notes = format!("{} Probe error: {}", notes, e);
                FoundryLocalPhase::Error
            }
        }
    } else {
        // Determine phase without probe
        backend.phase()
    };

    Ok(FoundryLocalStatus {
        cli_available,
        service_running,
        service_url,
        models,
        configured_model,
        selected_model: backend.selected_model(),
        notes,
        phase,
        probe: backend.probe_snapshot(),
    })
}

/// Make Foundry Local ready (start service if needed + keep probing until ready or timeout).
#[tauri::command]
pub async fn make_foundry_ready(state: State<'_, AppState>) -> Result<FoundryLocalStatus, String> {
    use crate::llm::{FAST_PROBE_TIMEOUT_MS, SLOW_PROBE_TIMEOUT_MS};

    let config = {
        let guard = lock_or_recover(&state.config);
        guard.translation.foundry_local.clone()
    };
    if let Some(status) = managed_hy_mt_status(&config, true).await {
        return Ok(status);
    }
    let configured_model = config.model.clone();
    let configured_timeout_ms = config.timeout_ms as u64;
    let steady_probe_timeout_ms = configured_timeout_ms.clamp(5_000, SLOW_PROBE_TIMEOUT_MS);

    let backend = Arc::new(FoundryLocalBackend::new(config));

    let started = Instant::now();
    let max_total = Duration::from_secs(90);

    let mut cli_available = false;
    let mut service_url: Option<String> = None;
    let mut service_running = false;
    let mut models: Vec<String> = Vec::new();
    let mut notes = String::new();

    let mut last_error: Option<String> = None;
    let mut phase = FoundryLocalPhase::Preparing;
    let mut attempt = 0usize;
    let mut models_wait_started: Option<Instant> = None;

    while started.elapsed() < max_total {
        let (snap_cli, snap_url, snap_running, snap_models, snap_notes) =
            async_runtime::spawn_blocking({
                let backend = backend.clone();
                move || {
                    backend.refresh_service_status();
                    let cli_available = FoundryLocalBackend::is_cli_available();
                    let service_url = FoundryLocalBackend::get_service_url_from_cli();
                    let service_running = service_url.is_some();
                    let models = if service_running {
                        FoundryLocalBackend::get_cached_models_from_cli()
                    } else {
                        Vec::new()
                    };
                    let notes = backend.notes();
                    (cli_available, service_url, service_running, models, notes)
                }
            })
            .await
            .map_err(|err| format!("Foundry Local make-ready snapshot failed: {}", err))?;

        cli_available = snap_cli;
        service_url = snap_url;
        service_running = snap_running;
        models = snap_models;
        notes = snap_notes;

        if !cli_available {
            phase = FoundryLocalPhase::NotInstalled;
            break;
        }

        if !service_running {
            phase = FoundryLocalPhase::NotRunning;

            // Attempt to start the service (non-fatal if it takes time).
            let _ = async_runtime::spawn_blocking({
                let backend = backend.clone();
                move || backend.ensure_service_running()
            })
            .await;

            tokio::time::sleep(Duration::from_millis(900)).await;
            continue;
        }

        if models.is_empty() {
            phase = FoundryLocalPhase::NoModels;
            models_wait_started.get_or_insert_with(Instant::now);
            if models_wait_started
                .as_ref()
                .is_some_and(|t| t.elapsed() > Duration::from_secs(12))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(900)).await;
            continue;
        }
        models_wait_started = None;

        // Service + models exist: warm up the selected model and keep probing until Ready.
        // Note: probe_chat_completions() handles stabilization delay internally.
        attempt += 1;
        let timeout_ms = if attempt == 1 {
            SLOW_PROBE_TIMEOUT_MS
        } else {
            steady_probe_timeout_ms.max(FAST_PROBE_TIMEOUT_MS)
        };

        match backend.probe_chat_completions(timeout_ms).await {
            Ok(true) => {
                phase = FoundryLocalPhase::Ready;
                last_error = None;
                break;
            }
            Ok(false) => {
                phase = FoundryLocalPhase::Preparing;
            }
            Err(e) => {
                phase = FoundryLocalPhase::Error;
                last_error = Some(e.to_string());
                // Connection reset often means Foundry crashed; it will restart on a new port.
                // The global stabilization tracking in probe_chat_completions will handle
                // the delay when refresh_service_status() detects the new URL.
            }
        }

        // Give Foundry some breathing room; it may restart on a new port after a crash.
        tokio::time::sleep(Duration::from_millis(1500)).await;
    }

    if phase != FoundryLocalPhase::Ready {
        if let Some(err) = last_error {
            notes = format!("{} Last error: {}", notes, err);
        }
    }

    Ok(FoundryLocalStatus {
        cli_available,
        service_running,
        service_url,
        models,
        configured_model,
        selected_model: backend.selected_model(),
        notes,
        phase,
        probe: backend.probe_snapshot(),
    })
}

async fn managed_hy_mt_status(
    config: &crate::config::FoundryLocalConfig,
    start_if_needed: bool,
) -> Option<FoundryLocalStatus> {
    let runtime = config.managed_runtime.as_ref()?;
    let executable_ready = PathBuf::from(&runtime.executable_path).is_file();
    let expected_model_size = crate::engine_manifest::EngineManifest::shipped()
        .map(|manifest| manifest.model.artifact.size_bytes)
        .unwrap_or_default();
    let model_ready = PathBuf::from(&runtime.model_path)
        .metadata()
        .map(|metadata| expected_model_size > 0 && metadata.len() == expected_model_size)
        .unwrap_or(false);
    let service_running = if executable_ready && model_ready {
        if start_if_needed {
            hy_mt_runtime::ensure_ready(runtime, Duration::from_secs(90))
                .await
                .is_ok()
        } else {
            hy_mt_runtime::is_healthy(runtime).await
        }
    } else {
        false
    };
    let phase = if !executable_ready {
        FoundryLocalPhase::NotInstalled
    } else if !model_ready {
        FoundryLocalPhase::NoModels
    } else if service_running {
        FoundryLocalPhase::Ready
    } else {
        FoundryLocalPhase::NotRunning
    };
    let notes = match phase {
        FoundryLocalPhase::Ready => "Local Translation Engine is ready.".to_string(),
        FoundryLocalPhase::NotInstalled => "Translation runtime is missing.".to_string(),
        FoundryLocalPhase::NoModels => "HY-MT model is missing or incomplete.".to_string(),
        FoundryLocalPhase::NotRunning => "Translation engine is installed but stopped.".to_string(),
        _ => "Local Translation Engine is configured.".to_string(),
    };

    Some(FoundryLocalStatus {
        cli_available: executable_ready,
        service_running,
        service_url: Some(hy_mt_runtime::endpoint_url(runtime)),
        models: config
            .model
            .clone()
            .into_iter()
            .filter(|_| model_ready)
            .collect(),
        configured_model: config.model.clone(),
        selected_model: config.model.clone(),
        notes,
        phase,
        probe: None,
    })
}

/// Build Foundry Local status without performing a network probe (fast initial load).
fn build_foundry_local_status_no_probe(
    config: crate::config::FoundryLocalConfig,
) -> FoundryLocalStatus {
    let configured_model = config.model.clone();
    let backend = FoundryLocalBackend::new(config);
    backend.refresh_service_status();
    let cli_available = FoundryLocalBackend::is_cli_available();
    let service_url = FoundryLocalBackend::get_service_url_from_cli();
    let service_running = service_url.is_some();

    // Get cached models from CLI (synchronous)
    let models = if service_running {
        FoundryLocalBackend::get_cached_models_from_cli()
    } else {
        Vec::new()
    };

    // Determine phase without probe
    let phase = backend.phase();

    FoundryLocalStatus {
        cli_available,
        service_running,
        service_url,
        models,
        configured_model,
        selected_model: backend.selected_model(),
        notes: backend.notes(),
        phase,
        probe: backend.probe_snapshot(),
    }
}
// =============================================================================
// SETTINGS COMMANDS
// =============================================================================

/// Get the current app settings
///
/// Called from JavaScript: `const settings = await invoke('get_settings');`
#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppConfig, String> {
    state.startup_gate.wait_until_ready().await?;
    info!("Getting settings...");
    let config = lock_or_recover(&state.config);
    Ok(config.clone())
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
    info!("Saving settings...");

    let mut updated = settings.clone();

    // Persist the last capture region into the config
    let last_region = *lock_or_recover(&state.capture_region);
    updated.last_capture_region = last_region;

    // Persist the DPI scale factor so restored regions capture the correct physical pixels.
    let last_scale_factor = *lock_or_recover(&state.capture_scale_factor);
    updated.last_capture_scale_factor = Some(last_scale_factor);

    // Through the same path the window events use, rather than reading the
    // window here: that copy lacked the minimised and offscreen guards, so
    // saving any setting while minimised stored the minimised geometry.
    if let Some(window) = app.get_webview_window("main") {
        crate::window_lifecycle::remember_main_geometry(&window.as_ref().window());
        updated.window_preferences = lock_or_recover(&state.config).window_preferences.clone();
    }

    // Update the in-memory config
    {
        let mut config = lock_or_recover(&state.config);
        updated
            .translation
            .foundry_local
            .preserve_managed_runtime_from(&config.translation.foundry_local);
        *config = updated.clone();
    }

    // Persist to disk without blocking the UI thread
    let app_handle = app.clone();
    let updated_clone = updated.clone();
    async_runtime::spawn_blocking(move || save_config(&app_handle, &updated_clone))
        .await
        .map_err(|err| {
            let message = format!("Failed to spawn save_config task: {}", err);
            warn!("{}", message);
            message
        })?
        .map_err(|err| {
            let message = format!("Failed to save settings: {}", err);
            warn!("{}", message);
            message
        })?;

    Ok(())
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

    // Validate the region
    if width <= 0 || height <= 0 {
        return Err("Width and height must be positive".to_string());
    }
    if scale_factor <= 0.0 {
        return Err("Scale factor must be positive".to_string());
    }

    let region = CaptureRegion {
        x,
        y,
        width,
        height,
    };

    let mut capture_region = lock_or_recover(&state.capture_region);
    *capture_region = Some(region);

    let mut capture_scale_factor = lock_or_recover(&state.capture_scale_factor);
    *capture_scale_factor = scale_factor;
    state.pipeline_clock.invalidate_capture();

    Ok(())
}

/// Get the current capture region (if set)
///
/// Called from JavaScript: `const region = await invoke('get_capture_region');`
#[tauri::command]
pub fn get_capture_region(state: State<'_, AppState>) -> Option<CaptureRegion> {
    let region = lock_or_recover(&state.capture_region);
    *region
}

/// A full-screen "desktop snapshot" for the area selector background.
///
/// This is a workaround for transparency regressions: instead of relying on the webview
/// to be truly transparent, we render a screenshot behind the selection UI.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectorSnapshot {
    /// `data:image/png;base64,...`
    pub data_url: String,
    /// Snapshot width in physical pixels.
    pub width: i32,
    /// Snapshot height in physical pixels.
    pub height: i32,
}

/// Result from `open_area_selector` so the UI can show whether we used WinUI OverlayHost
/// or fell back to the legacy webview selector.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AreaSelectorMode {
    Winui,
    Legacy,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAreaSelectorResult {
    pub mode: AreaSelectorMode,
}

/// Open the area selector overlay window
///
/// Called from JavaScript: `await invoke('open_area_selector');`
#[tauri::command]
pub async fn open_area_selector(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<OpenAreaSelectorResult, String> {
    // We currently prefer the "legacy" webview-based selector because it has a stable
    // desktop-snapshot background and correct DPI mapping.
    //
    // WinUI selector is kept as an opt-in experiment for future work.
    let prefer_winui = env_truthy("MEOWCAL_USE_WINUI_SELECTOR");

    if prefer_winui {
        info!("🎯 Opening area selector via OverlayHost (opt-in)");

        if let Some(ipc_server) = app.try_state::<Arc<IpcServer>>() {
            // On startup, the WinUI OverlayHost can take a moment to launch and connect.
            // Wait briefly so the first click is more likely to use WinUI instead of the legacy UI.
            const WAIT_MS: u64 = 2500;
            const STEP_MS: u64 = 50;
            let message = IpcMessage::new("Region.RequestOpenSelector");

            let mut waited = 0u64;
            while waited <= WAIT_MS {
                if ipc_server.is_connected() && ipc_server.send(message.clone()).await {
                    return Ok(OpenAreaSelectorResult {
                        mode: AreaSelectorMode::Winui,
                    });
                }

                tokio::time::sleep(Duration::from_millis(STEP_MS)).await;
                waited = waited.saturating_add(STEP_MS);
            }

            warn!("⚠️ OverlayHost not connected; falling back to legacy selector");
        } else {
            warn!("⚠️ IPC server not initialized; falling back to legacy selector");
        }
    }

    open_area_selector_legacy(app, state).await?;
    Ok(OpenAreaSelectorResult {
        mode: AreaSelectorMode::Legacy,
    })
}

/// Legacy area selector (kept for fallback if WinUI3 is not available)
#[allow(dead_code)]
async fn open_area_selector_legacy(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    info!("Opening area selector...");

    if let Some(window) = app.get_webview_window("selector") {
        // Capture a background snapshot BEFORE showing the selector window.
        // If we capture after showing, the screenshot will include the selector UI itself.
        //
        // This is best-effort: if capture fails, we still show the selector (it will just be grey).
        match async_runtime::spawn_blocking(|| -> Result<(SelectorSnapshot, bool), String> {
            use base64::Engine;

            // 1) Capture the primary screen in physical pixels.
            let (width, height) = capture::get_screen_dimensions();
            if width <= 0 || height <= 0 {
                return Err(format!("Invalid screen dimensions: {}x{}", width, height));
            }

            let region = CaptureRegion::new(0, 0, width, height);
            let (capture, used_fallback) =
                capture::smart_capture(&region).map_err(|e| format!("{}", e))?;

            // 2) Convert BGRA -> RGBA (swap red/blue channels).
            // Our capture backends return BGRA to match Windows APIs.
            let mut rgba = capture.data;
            for px in rgba.chunks_exact_mut(4) {
                px.swap(0, 2);
            }

            // 3) Encode to PNG.
            let mut png_bytes = Vec::new();
            {
                let mut encoder = png::Encoder::new(&mut png_bytes, capture.width, capture.height);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);

                let mut writer = encoder
                    .write_header()
                    .map_err(|e| format!("PNG header write failed: {}", e))?;

                writer
                    .write_image_data(&rgba)
                    .map_err(|e| format!("PNG encoding failed: {}", e))?;
            }

            // 4) Base64 encode for the webview (<img src="data:...">).
            let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
            let data_url = format!("data:image/png;base64,{}", b64);

            Ok((
                SelectorSnapshot {
                    data_url,
                    width,
                    height,
                },
                used_fallback,
            ))
        })
        .await
        {
            Ok(Ok((snapshot, used_fallback))) => {
                if used_fallback {
                    warn!("Area selector snapshot: Graphics Capture failed, used GDI fallback");
                } else {
                    info!("Area selector snapshot: captured via Graphics Capture");
                }

                // Store for the selector window to pull on load (and for subsequent opens).
                *lock_or_recover(&state.selector_snapshot) = Some(snapshot.clone());

                // Also push it as an event (helps if the selector window is already loaded).
                let _ = window.emit("selector-background-snapshot", snapshot);
            }
            Ok(Err(e)) => {
                warn!("Area selector snapshot capture failed: {}", e);
                *lock_or_recover(&state.selector_snapshot) = None;
            }
            Err(join_err) => {
                warn!("Area selector snapshot task failed: {}", join_err);
                *lock_or_recover(&state.selector_snapshot) = None;
            }
        }

        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        info!("✅ Area selector opened!");
    } else {
        return Err("Selector window not found".to_string());
    }

    Ok(())
}

/// Get the most recent selector background snapshot (if available).
///
/// Called from JavaScript (selector window): `const snap = await invoke('get_selector_snapshot');`
#[tauri::command]
pub fn get_selector_snapshot(state: State<'_, AppState>) -> Option<SelectorSnapshot> {
    lock_or_recover(&state.selector_snapshot).clone()
}

/// Close the area selector overlay window
///
/// Called from JavaScript: `await invoke('close_area_selector');`
#[tauri::command]
pub async fn close_area_selector(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    info!("Closing area selector...");

    if let Some(window) = app.get_webview_window("selector") {
        window.hide().map_err(|e| e.to_string())?;
        // Drop the snapshot to avoid holding a huge base64 string in memory.
        *lock_or_recover(&state.selector_snapshot) = None;
        info!("✅ Area selector closed!");
    } else {
        return Err("Selector window not found".to_string());
    }

    Ok(())
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

struct CompressionFlagGuard {
    flag: Arc<AtomicBool>,
}

impl CompressionFlagGuard {
    fn new(flag: Arc<AtomicBool>) -> Self {
        Self { flag }
    }
}

impl Drop for CompressionFlagGuard {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::SeqCst);
    }
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

    if let Err(e) = overlay::show_overlay(&app) {
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
    let compression_in_flight = Arc::new(AtomicBool::new(false));
    let context_generation = Arc::new(AtomicU64::new(0));
    let last_summary_scheduled_ms = Arc::new(AtomicU64::new(0));

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

        let mut last_text = String::new();
        let mut last_attempt_at = Instant::now()
            .checked_sub(Duration::from_millis(MOCK_RETRY_COOLDOWN_MS))
            .unwrap_or_else(Instant::now);
        let mut last_capture_region: Option<CaptureRegion> = None;
        let mut empty_ocr_frames: u32 = 0;
        // The loop runs several times a second. Re-emitting the same notice every
        // pass would flood the overlay, so only a change of reason is reported.
        let mut last_notice: Option<&'static str> = None;

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
                last_text.clear();
                translator.set_last_backend_was_mock(true);
                last_attempt_at = Instant::now()
                    .checked_sub(Duration::from_millis(MOCK_RETRY_COOLDOWN_MS))
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
                debug!("[FILTER: empty] No text detected, skipping");
                empty_ocr_frames = empty_ocr_frames.saturating_add(1);

                // A subtitle that leaves the region must leave the overlay too.
                // Waiting a few frames keeps single-frame OCR misses from
                // flickering the line away between subtitle changes.
                if empty_ocr_frames >= EMPTY_OCR_CLEAR_FRAMES
                    && last_notice != Some("empty")
                    && !translator.is_busy()
                {
                    last_notice = Some("empty");
                    // The cleared line has to be translatable again: without this
                    // the duplicate filter would suppress the identical subtitle
                    // when it returns, leaving the overlay permanently blank.
                    last_text.clear();
                    let _ = app.emit(
                        "translation-update",
                        TranslationPayload::no_subtitle_text(session_id, token.capture_id),
                    );
                }

                tokio::time::sleep(pacer.remaining_for(frame_started)).await;
                continue;
            }

            empty_ocr_frames = 0;

            let current_text = ocr_result.text.trim().to_string();

            if let Some(rejection) = crate::ocr_gate::classify(&current_text, min_significant_chars)
            {
                debug!(
                    "[FILTER: {}] OCR text ({} chars, minimum {})",
                    rejection.as_str(),
                    current_text.chars().count(),
                    min_significant_chars
                );
                // Text *is* in the region, so the overlay must stop claiming the
                // region is empty. Staying silent leaves whichever notice is on
                // screen contradicting what the viewer can see.
                if last_notice != Some(rejection.as_str()) && !translator.is_busy() {
                    last_notice = Some(rejection.as_str());
                    let _ = app.emit(
                        "translation-update",
                        TranslationPayload::source_unreadable(
                            session_id,
                            token.capture_id,
                            rejection,
                        ),
                    );
                }
                tokio::time::sleep(pacer.remaining_for(frame_started)).await;
                continue;
            }

            let now = Instant::now();
            // Exact equality alone treated OCR noise - a dropped glyph, a comma
            // read as a period - as fresh dialogue, so one subtitle on screen
            // earned two or three translations in a row.
            let line_change = crate::ocr_stability::classify(&last_text, &current_text);
            let mut force_retry_duplicate = false;
            if line_change == crate::ocr_stability::LineChange::Repeat {
                if !translator.last_backend_was_mock() {
                    debug!(source = %current_text, "[FILTER: duplicate_line] OCR text");
                    translation_manager.record_ocr_line(&current_text);
                    tokio::time::sleep(pacer.remaining_for(frame_started)).await;
                    continue;
                }

                if now.duration_since(last_attempt_at)
                    < Duration::from_millis(MOCK_RETRY_COOLDOWN_MS)
                {
                    debug!("[FILTER: duplicate_mock_cooldown] OCR text");
                    translation_manager.record_ocr_line(&current_text);
                    tokio::time::sleep(pacer.remaining_for(frame_started)).await;
                    continue;
                }

                force_retry_duplicate = true;
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

            last_text = current_text.clone();
            last_notice = None;
            last_attempt_at = now;
            // Bumped here, not in the task: see `Translator`.
            context_generation.fetch_add(1, Ordering::SeqCst);
            info!("📝 OCR detected ({} chars)", current_text.chars().count());
            translation_manager.record_ocr_line(&current_text);

            // Check if context needs compression (async, don't block).
            //
            // We throttle + delay the summarization so it runs during stable subtitle windows,
            // reducing contention with the live translation loop.
            if translation_manager.needs_context_compression() {
                let now_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let cooldown_ms = translation_config_for_summary.context_summary_cooldown_ms as u64;
                let last_scheduled = last_summary_scheduled_ms.load(Ordering::SeqCst);
                let cooldown_ok =
                    cooldown_ms == 0 || now_ms.saturating_sub(last_scheduled) >= cooldown_ms;

                if cooldown_ok && !compression_in_flight.swap(true, Ordering::SeqCst) {
                    last_summary_scheduled_ms.store(now_ms, Ordering::SeqCst);
                    debug!("Context needs compression, scheduling summarization");

                    let manager = Arc::clone(&translation_manager);
                    let config = translation_config_for_summary.clone();
                    let compression_flag = Arc::clone(&compression_in_flight);
                    let stop_rx_for_summary = stop_rx.clone();
                    let generation = Arc::clone(&context_generation);
                    let scheduled_generation = generation.load(Ordering::SeqCst);

                    tokio::spawn(async move {
                        let _reset = CompressionFlagGuard::new(compression_flag);

                        tokio::time::sleep(Duration::from_millis(
                            CONTEXT_SUMMARY_STABILITY_DELAY_MS,
                        ))
                        .await;

                        if *stop_rx_for_summary.borrow() {
                            return;
                        }

                        // Abort if subtitles changed while we were waiting for an idle window.
                        if generation.load(Ordering::SeqCst) != scheduled_generation {
                            debug!("Skipping context summarization (text still changing)");
                            return;
                        }

                        let history_entries = manager.get_history_for_summarization();
                        if history_entries.is_empty() {
                            return;
                        }

                        if !config.enable_foundry_local {
                            manager.restore_history_entries(history_entries);
                            manager.cap_history_to_budget();
                            return;
                        }

                        let history_lines: Vec<String> = history_entries
                            .iter()
                            .map(|entry| entry.text.clone())
                            .collect();

                        // Use a fresh backend instance for each summarization run to ensure a clean prompt
                        // (no shared chat/session state), but make sure we refresh service discovery before use.
                        let backend = FoundryLocalBackend::new(config.foundry_local.clone());
                        backend.refresh_service_status();

                        if !backend.is_available() {
                            manager.restore_history_entries(history_entries);
                            manager.cap_history_to_budget();
                            return;
                        }

                        for attempt in 1..=CONTEXT_SUMMARY_MAX_RETRIES {
                            if *stop_rx_for_summary.borrow() {
                                return;
                            }
                            match backend.summarize_context(&history_lines).await {
                                Ok(summary) if !summary.trim().is_empty() => {
                                    manager.update_context_memory(summary);
                                    return;
                                }
                                Ok(_) => {
                                    warn!(
                                        "Context summarization attempt {} returned empty output",
                                        attempt
                                    );
                                }
                                Err(err) => {
                                    warn!(
                                        "Context summarization attempt {} failed: {}",
                                        attempt, err
                                    );
                                }
                            }

                            if attempt == CONTEXT_SUMMARY_MAX_RETRIES {
                                manager.restore_history_entries(history_entries);
                                manager.cap_history_to_budget();
                                return;
                            }

                            tokio::time::sleep(Duration::from_millis(
                                CONTEXT_SUMMARY_RETRY_DELAY_MS,
                            ))
                            .await;
                        }
                    });
                }
            }

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
    use tauri::Emitter;
    info!("Opening Foundry setup wizard");
    if let Some(window) = app.get_webview_window("foundry-wizard") {
        // Emit reset event so the wizard JS resets to step 1 and clears timers
        let _ = window.emit("wizard-reset", ());
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        window.center().map_err(|e| e.to_string())?;
    } else {
        return Err("Wizard window not found".to_string());
    }
    Ok(())
}

/// Hide the foundry-wizard window and notify the main window
#[tauri::command]
pub fn close_foundry_wizard(
    app: AppHandle,
    model_downloaded: bool,
    selected_model: Option<String>,
) -> Result<(), String> {
    info!("Closing Foundry setup wizard");
    if let Some(window) = app.get_webview_window("foundry-wizard") {
        window.hide().map_err(|e| e.to_string())?;
    }
    // Notify main window so it can refresh status and auto-configure
    let _ = app.emit(
        "foundry-wizard-closed",
        serde_json::json!({
            "modelDownloaded": model_downloaded,
            "selectedModel": selected_model,
        }),
    );
    Ok(())
}

#[tauri::command]
pub async fn wizard_install_engine(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let manifest =
        crate::engine_manifest::EngineManifest::shipped().map_err(|error| error.to_string())?;
    let cache_dir = {
        let config = lock_or_recover(&state.config);
        config
            .translation
            .foundry_local
            .managed_cache_root()
            .map(|path| path.to_string_lossy().to_string())
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
