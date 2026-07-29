// =============================================================================
// COMMANDS.RS - Tauri IPC Commands
// =============================================================================
// These are functions that JavaScript can call from the web UI!
//
// The #[tauri::command] attribute is magic - it automatically:
// 1. Converts JavaScript arguments to Rust types
// 2. Converts Rust return values to JavaScript types
// 3. Handles errors gracefully
//
// In JavaScript, you call these like:
//   const result = await invoke('get_settings');
//   await invoke('save_settings', { settings: { ... } });
// =============================================================================

use crate::capture;
use crate::config::{save_config, AppConfig, CaptureRegion};
use crate::event_payloads::{CaptureStatusPayload, TranslationPayload};
use crate::ipc::{
    IpcMessage, IpcServer, OverlaySettingsData, RegionData, SetRegionPayload, SettingsSyncPayload,
    SubtitleUpdatePayload,
};
use crate::llm::{
    BackendId, BackendInfo, FoundryLocalBackend, FoundryLocalPhase, TranslationDiagnostics,
    TranslationDiagnosticsState, TranslationDisplayState, TranslationManager, TranslationOutcome,
    TranslatorBackend,
};
use crate::ocr::{PreprocessingConfig, WindowsOcr};
use crate::overlay;
use crate::sync_utils::lock_or_recover;
use reqwest::Client;
use scopeguard::defer;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{async_runtime, AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tokio::fs;
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
async fn send_overlay_message(app: &AppHandle, message: IpcMessage) {
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

// =============================================================================
// APP STATE
// =============================================================================
// This holds the current state of our application.
// It's shared across all commands and persists while the app is running.

/// The application state, managed by Tauri
pub struct AppState {
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
            config: Mutex::new(AppConfig::default()),
            is_running: Mutex::new(false),
            capture_region: Mutex::new(None),
            capture_scale_factor: Mutex::new(1.0),
            stop_signal: Mutex::new(None),
            translation_diagnostics: Arc::new(Mutex::new(TranslationDiagnosticsState::default())),
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

/// An available translateLocally download option for this platform.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateLocallyDownloadOption {
    pub id: String,
    pub label: String,
    pub asset_name: String,
    pub url: String,
    pub notes: String,
}

/// Download info for translateLocally (recommended build + options).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateLocallyDownloadInfo {
    pub recommended_id: String,
    pub default_install_dir: String,
    pub options: Vec<TranslateLocallyDownloadOption>,
}

/// Result after a translateLocally download attempt.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateLocallyDownloadResult {
    pub path: String,
    pub option_id: String,
    pub used_fallback: bool,
    pub notes: String,
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
// DOWNLOAD COMMANDS (OFFLINE MT)
// =============================================================================

const TRANSLATE_LOCALLY_BASE_URL: &str =
    "https://github.com/XapaJIaMnu/translateLocally/releases/download/latest";

/// Open the translateLocally download page in the default browser.
#[tauri::command]
pub fn open_translate_locally_download(app: AppHandle) -> Result<(), String> {
    let url = "https://github.com/XapaJIaMnu/translateLocally/releases/tag/latest";
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Get recommended translateLocally download options for this machine.
#[tauri::command]
pub fn get_translate_locally_download_info(
    app: AppHandle,
) -> Result<TranslateLocallyDownloadInfo, String> {
    build_translate_locally_download_info(&app)
}

/// Download translateLocally and return the installed binary path.
#[tauri::command]
pub async fn download_translate_locally(
    app: AppHandle,
    option_id: Option<String>,
    install_dir: String,
) -> Result<TranslateLocallyDownloadResult, String> {
    let download_info = build_translate_locally_download_info(&app)?;
    let mut options = download_info.options;

    if options.is_empty() {
        return Err("No translateLocally builds available for this platform.".to_string());
    }

    // Pick the requested option or fall back to the recommended one.
    let requested = option_id
        .and_then(|id| {
            let trimmed = id.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
        .unwrap_or_else(|| download_info.recommended_id.clone());

    let mut order = Vec::new();
    if let Some(index) = options.iter().position(|opt| opt.id == requested) {
        let option = options.remove(index);
        order.push(option);
    }
    order.extend(options);

    // Resolve the install path (folder or full file path).
    let target_path = resolve_install_target(&app, &install_dir)?;
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create install dir: {}", e))?;
    }
    let mut last_error = None;

    // Try the requested build first, then fall back if the download fails.
    for (idx, option) in order.iter().enumerate() {
        let result = download_translate_locally_asset(&option.url, &target_path).await;
        match result {
            Ok(_) => {
                let used_fallback = idx > 0;
                let notes = if used_fallback {
                    format!("Downloaded fallback build: {}", option.label)
                } else {
                    format!("Downloaded: {}", option.label)
                };
                return Ok(TranslateLocallyDownloadResult {
                    path: target_path.to_string_lossy().to_string(),
                    option_id: option.id.clone(),
                    used_fallback,
                    notes,
                });
            }
            Err(err) => {
                last_error = Some(err);
                continue;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| "Download failed.".to_string()))
}

// =============================================================================
// FOUNDRY LOCAL COMMANDS
// =============================================================================

/// Foundry Local service status
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
    let config = {
        let guard = lock_or_recover(&state.config);
        guard.translation.foundry_local.clone()
    };

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
fn build_translate_locally_download_info(
    app: &AppHandle,
) -> Result<TranslateLocallyDownloadInfo, String> {
    let options = translate_locally_options()?;
    if options.is_empty() {
        return Err("No translateLocally builds found for this platform.".to_string());
    }

    let recommended_id = options
        .first()
        .map(|opt| opt.id.clone())
        .unwrap_or_else(|| "unknown".to_string());

    Ok(TranslateLocallyDownloadInfo {
        recommended_id,
        default_install_dir: default_translate_locally_dir(app)
            .to_string_lossy()
            .to_string(),
        options,
    })
}

fn translate_locally_options() -> Result<Vec<TranslateLocallyDownloadOption>, String> {
    if std::env::consts::OS != "windows" {
        return Err("In-app download is only available on Windows.".to_string());
    }

    let arch = std::env::consts::ARCH;
    let mut options = Vec::new();

    // Prefer AVX build on x86_64, but stay compatible for ARM64 and older CPUs.
    if arch == "aarch64" {
        options.push(build_option(
            "win-x64",
            "Windows x64 (non-AVX) - recommended for ARM64",
            "translateLocally.windows-2019.x86-64.exe",
            "Runs under x64 emulation. AVX builds will not run on ARM64.",
        ));
    } else if arch == "x86_64" {
        if supports_avx() {
            options.push(build_option(
                "win-avx",
                "Windows x64 (AVX optimized)",
                "translateLocally.windows-2022.core-avx-i.exe",
                "Fastest option if your CPU supports AVX.",
            ));
        }

        options.push(build_option(
            "win-x64",
            "Windows x64 (non-AVX)",
            "translateLocally.windows-2019.x86-64.exe",
            "Most compatible option for older CPUs.",
        ));
    } else {
        return Err(format!("Unsupported CPU architecture: {}", arch));
    }

    Ok(options)
}

fn build_option(
    id: &str,
    label: &str,
    asset_name: &str,
    notes: &str,
) -> TranslateLocallyDownloadOption {
    TranslateLocallyDownloadOption {
        id: id.to_string(),
        label: label.to_string(),
        asset_name: asset_name.to_string(),
        // Use the GitHub "latest/download" URL to avoid extra API calls.
        url: format!("{}/{}", TRANSLATE_LOCALLY_BASE_URL, asset_name),
        notes: notes.to_string(),
    }
}

fn supports_avx() -> bool {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        std::is_x86_feature_detected!("avx")
    }

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        false
    }
}

fn default_translate_locally_dir(app: &AppHandle) -> PathBuf {
    if cfg!(target_os = "windows") {
        if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(local_appdata).join("translateLocally");
        }
    }

    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("translateLocally")
}

fn resolve_install_target(app: &AppHandle, raw_input: &str) -> Result<PathBuf, String> {
    let trimmed = raw_input.trim();
    if trimmed.is_empty() {
        return Err("Install path is required.".to_string());
    }

    let path = PathBuf::from(trimmed);
    let mut resolved = if path.is_absolute() {
        path
    } else {
        app.path()
            .app_data_dir()
            .map_err(|e| format!("Failed to resolve app data dir: {}", e))?
            .join(path)
    };

    // If the user already provided a file path, keep it.
    if resolved.extension().is_some() {
        return Ok(resolved);
    }

    resolved.push(default_translate_locally_filename());
    Ok(resolved)
}

fn default_translate_locally_filename() -> &'static str {
    if cfg!(target_os = "windows") {
        "translateLocally.exe"
    } else {
        "translateLocally"
    }
}

async fn download_translate_locally_asset(url: &str, target_path: &PathBuf) -> Result<(), String> {
    let client = Client::builder()
        .user_agent("Meowcal-Sub/0.1.0")
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Download request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Download failed: HTTP {}", response.status()));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read download: {}", e))?;

    fs::write(target_path, &bytes)
        .await
        .map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(())
}

// =============================================================================
// SETTINGS COMMANDS
// =============================================================================

/// Get the current app settings
///
/// Called from JavaScript: `const settings = await invoke('get_settings');`
#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppConfig {
    info!("Getting settings...");
    let config = lock_or_recover(&state.config);
    config.clone()
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

    // Capture window preferences if possible
    if let Some(window) = app.get_webview_window("main") {
        if let Ok(size) = window.inner_size() {
            updated.window_preferences.width = Some(size.width);
            updated.window_preferences.height = Some(size.height);
        }
        if let Ok(position) = window.outer_position() {
            updated.window_preferences.x = Some(position.x);
            updated.window_preferences.y = Some(position.y);
        }
        if let Ok(is_maximized) = window.is_maximized() {
            updated.window_preferences.is_maximized = is_maximized;
        }
    }

    // Update the in-memory config
    {
        let mut config = lock_or_recover(&state.config);
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

/// Start the translation process
///
/// This will:
/// 1. Capture the screen region periodically
/// 2. Run OCR on each capture
/// 3. Translate the recognized text
/// 4. Send results back to the overlay UI via events
///
/// Called from JavaScript: `await invoke('start_translation');`
#[tauri::command]
pub async fn start_translation(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    info!(">>> START_TRANSLATION COMMAND CALLED <<<");
    info!("Starting translation...");

    // Get the capture region
    let region = {
        let region_guard = lock_or_recover(&state.capture_region);
        match *region_guard {
            Some(r) => r,
            None => return Err("No capture region set. Please select an area first.".to_string()),
        }
    };

    // Mark as running only after we know we have a region (prevents "stuck running" on early return).
    {
        let mut is_running = lock_or_recover(&state.is_running);
        if *is_running {
            return Err("Translation is already running".to_string());
        }
        *is_running = true;
    }

    // Get the capture scale factor (logical -> physical pixels)
    let scale_factor = {
        let scale_guard = lock_or_recover(&state.capture_scale_factor);
        *scale_guard
    };
    let capture_region = region.scaled(scale_factor);
    debug!(
        "Capture scale factor: {}, logical: {:?}, physical: {:?}",
        scale_factor, region, capture_region
    );

    // Get settings from config
    let (interval_ms, source_language, target_language, translation_config) = {
        let config = lock_or_recover(&state.config);
        (
            config.capture_interval_ms,
            config.source_language.clone(),
            config.target_language.clone(),
            config.translation.clone(),
        )
    };

    // Extract OCR-specific settings
    let ocr_confidence_threshold = translation_config.ocr.confidence_threshold;
    let ocr_preprocessing_enabled = translation_config.ocr.preprocessing_enabled;
    let ocr_grayscale = translation_config.ocr.grayscale;
    let ocr_contrast_enhancement = translation_config.ocr.contrast_enhancement;
    let ocr_binarize = translation_config.ocr.binarize;
    let ocr_enable_multi_pass = translation_config.ocr.enable_multi_pass;
    let ocr_multi_pass_count = translation_config.ocr.multi_pass_count;
    let ocr_validation_strictness = translation_config.ocr.validation_strictness;

    // Calculate effective confidence threshold: use explicit threshold if set,
    // otherwise fall back to strictness-based default.
    // Using max() ensures strictness acts as a minimum floor - the effective
    // threshold is always at least as strict as the strictness setting.
    let strictness_threshold = ocr_validation_strictness.threshold();
    let effective_confidence_threshold = if ocr_confidence_threshold > 0.0 {
        // If user explicitly set a threshold, use that (floored by strictness)
        ocr_confidence_threshold.max(strictness_threshold)
    } else {
        // Default to strictness-based threshold
        strictness_threshold
    };

    debug!(
        "OCR settings: confidence_threshold={:.2}, preprocessing={}, grayscale={}, contrast={}, binarize={}, multi_pass={}, pass_count={}, strictness={:?}, effective_threshold={:.2}",
        ocr_confidence_threshold,
        ocr_preprocessing_enabled,
        ocr_grayscale,
        ocr_contrast_enhancement,
        ocr_binarize,
        ocr_enable_multi_pass,
        ocr_multi_pass_count,
        ocr_validation_strictness,
        effective_confidence_threshold
    );

    let context_enabled = translation_config.enable_context_aware;
    let translation_config_for_summary = translation_config.clone();

    // Create a stop signal channel
    // The sender stays here, the receiver goes to the spawned task
    let (stop_tx, mut stop_rx) = watch::channel(false);

    // Store the sender so stop_translation can use it
    {
        let mut stop_signal = lock_or_recover(&state.stop_signal);
        *stop_signal = Some(stop_tx);
    }

    // Note: is_running is already set to true above (atomic check-and-set)

    info!(
        "✅ Translation started! Interval: {}ms, Target: {}",
        interval_ms, target_language
    );

    // Show the overlay and send the capture region to it (legacy WebView overlay)
    if let Err(e) = overlay::show_overlay(&app) {
        warn!("⚠️ Failed to show overlay: {}", e);
    }
    if let Err(e) = overlay::update_overlay_region(&app, &region) {
        warn!("⚠️ Failed to update overlay region: {}", e);
    }

    // Send messages to WinUI3 OverlayHost
    send_overlay_message(&app, IpcMessage::new("Overlay.Show")).await;

    // Send initial region if set
    let payload = SetRegionPayload {
        region: RegionData::from(&region),
    };
    send_overlay_message(&app, IpcMessage::with_payload("Overlay.SetRegion", payload)).await;

    // Send initial settings to overlay
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

    // Initialize translation backend manager
    let diagnostics = state.translation_diagnostics.clone();
    let translation_manager = Arc::new(TranslationManager::new(
        translation_config,
        app.clone(),
        diagnostics,
    ));
    let compression_in_flight = Arc::new(AtomicBool::new(false));
    let context_generation = Arc::new(AtomicU64::new(0));
    let last_summary_scheduled_ms = Arc::new(AtomicU64::new(0));

    // Clone app handle for use inside the async block to access state
    let app_for_region = app.clone();

    // Spawn the background translation loop with panic monitoring.
    // If the task panics, we log it and the scopeguard (defer!) still runs for cleanup.
    let translation_handle = tokio::spawn(async move {
        // Scope guard ensures is_running is reset even if the task panics (in debug builds).
        // This replaces the manual reset_running_state() calls with RAII-style cleanup.
        let app_for_guard = app.clone();
        defer! {
            let state = app_for_guard.state::<AppState>();
            // Use lock_or_recover to ensure cleanup succeeds even if mutex is poisoned
            *lock_or_recover(&state.is_running) = false;
            capture::close_capture_session();
            info!("Translation loop cleanup complete");
        }

        info!("Translation loop started");
        info!("Initializing OCR engine (language={})", source_language);

        // Ensure WinRT is initialized on this worker thread before calling OCR/capture APIs.
        // This prevents "nothing happens" failures when the runtime isn't set up on the thread.
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
            if let Err(e) = unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
                warn!("⚠️ Failed to initialize WinRT on worker thread: {}", e);
            }
        }

        // Initialize OCR engine using the configured source language
        let ocr = match WindowsOcr::with_language(&source_language) {
            Ok(o) => {
                info!("OCR initialized with language: {}", source_language);
                o
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
                    Ok(o) => o,
                    Err(e) => {
                        warn!("❌ Failed to initialize OCR: {}", e);
                        // Scopeguard handles cleanup automatically on early return
                        return;
                    }
                }
            }
        };

        // Keep track of last OCR text to avoid duplicate processing
        let mut last_text = String::new();
        let mut last_backend_used = BackendId::Mock;
        let mut last_attempt_at = Instant::now()
            .checked_sub(Duration::from_millis(MOCK_RETRY_COOLDOWN_MS))
            .unwrap_or_else(Instant::now);
        let mut last_capture_region: Option<CaptureRegion> = None;

        // Initialize capture session state
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

        loop {
            // Check if we should stop
            if *stop_rx.borrow() {
                info!("🛑 Stop signal received, exiting loop");
                break;
            }

            // Also check for channel changes (non-blocking)
            if stop_rx.has_changed().unwrap_or(false) {
                let _ = stop_rx.borrow_and_update();
                if *stop_rx.borrow() {
                    info!("🛑 Stop signal received, exiting loop");
                    break;
                }
            }

            // Re-read the capture region from state (allows live resize/reposition)
            let current_capture_region = {
                let state = app_for_region.state::<AppState>();
                let region_opt = *lock_or_recover(&state.capture_region);
                let scale = *lock_or_recover(&state.capture_scale_factor);
                // Mutex guards are dropped here before any await
                match region_opt {
                    Some(r) => {
                        let scaled = r.scaled(scale);
                        let (screen_width, screen_height) = capture::get_screen_dimensions();

                        if scaled.intersects_origin_bounds(screen_width, screen_height) {
                            scaled
                                .clamp_to_bounds(screen_width, screen_height)
                                .unwrap_or(scaled)
                        } else {
                            // Keep the original region if it's completely outside the primary screen.
                            // This preserves multi-monitor behavior for GDI fallback captures.
                            scaled
                        }
                    }
                    None => {
                        warn!("⚠️ No capture region set, skipping frame");
                        tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
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
                last_backend_used = BackendId::Mock;
                last_attempt_at = Instant::now()
                    .checked_sub(Duration::from_millis(MOCK_RETRY_COOLDOWN_MS))
                    .unwrap_or_else(Instant::now);
                // Keep this monotonic: resetting to 0 can allow old summarization tasks to
                // accidentally validate again once the counter reaches the same value.
                context_generation.fetch_add(1, Ordering::SeqCst);
            }
            last_capture_region = Some(current_capture_region);

            debug!("📸 Capturing region: {:?}", current_capture_region);

            // Step 1: Capture screen region with session fallback handling
            let capture_result =
                match try_capture(&current_capture_region, &mut capture_state, &app) {
                    CaptureAttemptResult::Success(result) => result,
                    CaptureAttemptResult::RetryAfterDelay => {
                        tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
                        continue;
                    }
                };

            // If stop was requested while we were capturing, don't do any more work or emit updates.
            if *stop_rx.borrow() {
                info!("🛑 Stop signal received, aborting current frame");
                break;
            }

            // Step 2: Run OCR
            let ocr_result = if ocr_enable_multi_pass {
                // Multi-pass OCR: run multiple passes with different preprocessing and pick best
                match ocr
                    .recognize_multi_pass(
                        &capture_result.data,
                        capture_result.width,
                        capture_result.height,
                        ocr_multi_pass_count,
                    )
                    .await
                {
                    Ok(result) => result,
                    Err(e) => {
                        warn!("⚠️ Multi-pass OCR failed: {}", e);
                        tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
                        continue;
                    }
                }
            } else if ocr_preprocessing_enabled {
                let preprocessing_config = PreprocessingConfig {
                    grayscale: ocr_grayscale,
                    contrast_enhancement: ocr_contrast_enhancement,
                    binarize: ocr_binarize,
                };
                match ocr
                    .recognize_with_preprocessing(
                        &capture_result.data,
                        capture_result.width,
                        capture_result.height,
                        preprocessing_config,
                    )
                    .await
                {
                    Ok(result) => result,
                    Err(e) => {
                        warn!("⚠️ OCR failed: {}", e);
                        tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
                        continue;
                    }
                }
            } else {
                match ocr
                    .recognize_without_preprocessing(
                        &capture_result.data,
                        capture_result.width,
                        capture_result.height,
                    )
                    .await
                {
                    Ok(result) => result,
                    Err(e) => {
                        warn!("⚠️ OCR failed: {}", e);
                        tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
                        continue;
                    }
                }
            };

            // If stop was requested while OCR was running, skip translation and exit cleanly.
            if *stop_rx.borrow() {
                info!("🛑 Stop signal received, aborting current frame");
                break;
            }

            // Skip if empty or same as last frame
            if ocr_result.is_empty() {
                debug!("[FILTER: empty] No text detected, skipping");
                tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
                continue;
            }

            // Get confidence score from OCR result
            let confidence = ocr_result.confidence.unwrap_or(0.0);
            let current_text = ocr_result.text.trim().to_string();

            // Verbose OCR logging: show first 100 chars of OCR output
            let text_preview: String = current_text.chars().take(100).collect();
            debug!(
                "OCR output ({} chars, confidence: {:.2}): {:?}",
                current_text.chars().count(),
                confidence,
                text_preview
            );

            // Confidence threshold check - skip if below threshold
            if confidence < effective_confidence_threshold {
                let preview: String = current_text.chars().take(40).collect();
                debug!(
                    "[FILTER: low_confidence] ({:.2} < {:.2}): {:?}",
                    confidence, effective_confidence_threshold, preview
                );
                tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
                continue;
            }

            let significant_chars = current_text
                .chars()
                .filter(|ch| ch.is_alphanumeric())
                .count();
            if significant_chars < 2 {
                debug!("[FILTER: very_short] Noise/very short text detected, skipping");
                tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
                continue;
            }

            // Additional garbage detection: skip OCR artifacts from credits, logos,
            // and scrambled character recognition that would cause hallucinations.
            // Filter reason: untranslatable text
            if crate::llm::is_untranslatable_text(&current_text) {
                let preview: String = current_text.chars().take(40).collect();
                debug!(
                    "[FILTER: untranslatable] OCR text ({} chars, confidence: {:.2}): {:?}",
                    current_text.chars().count(),
                    confidence,
                    preview
                );
                tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
                continue;
            }

            let now = Instant::now();
            let is_exact_duplicate = current_text == last_text;
            let mut force_retry_duplicate = false;
            if is_exact_duplicate {
                if last_backend_used != BackendId::Mock {
                    debug!(
                        "[FILTER: duplicate_exact] OCR text (confidence: {:.2})",
                        confidence
                    );
                    translation_manager.record_ocr_line(&current_text);
                    tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
                    continue;
                }

                // If we fell back to passthrough last time, retry occasionally so we can recover
                // once the LLM backend becomes ready (avoid spamming requests every frame).
                if now.duration_since(last_attempt_at)
                    < Duration::from_millis(MOCK_RETRY_COOLDOWN_MS)
                {
                    debug!(
                        "[FILTER: duplicate_mock_cooldown] OCR text (confidence: {:.2})",
                        confidence
                    );
                    translation_manager.record_ocr_line(&current_text);
                    tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
                    continue;
                }

                // Cooldown expired: allow a retry even if context dedup would normally skip this
                // line (otherwise we can get stuck in mock mode until OCR text changes).
                force_retry_duplicate = true;
            }

            if !force_retry_duplicate
                && context_enabled
                && translation_manager.is_duplicate(&current_text)
            {
                debug!(
                    "[FILTER: duplicate_context] OCR text (confidence: {:.2})",
                    confidence
                );
                translation_manager.record_ocr_line(&current_text);
                tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
                continue;
            }

            last_text = current_text.clone();
            context_generation.fetch_add(1, Ordering::SeqCst);
            last_attempt_at = now;
            info!("📝 OCR detected ({} chars)", current_text.chars().count());

            // Step 3: Translate via backend manager with fallback
            // Get context prompt if available
            let context_prompt = translation_manager.get_context_prompt();

            // Update subtitle context cache AFTER building the context prompt so we don't include
            // the current OCR line in the "recent lines" block (it will be translated separately).
            translation_manager.record_ocr_line(&current_text);

            let outcome = translation_manager
                .translate_with_context(
                    &current_text,
                    &source_language,
                    &target_language,
                    context_prompt.as_deref(),
                )
                .await;
            let TranslationOutcome {
                translated,
                backend_used,
                warnings,
                display_state,
            } = outcome;

            // If stop was requested while the translation backend was running, don't emit results.
            if *stop_rx.borrow() {
                info!("🛑 Stop signal received, discarding in-flight translation result");
                break;
            }

            last_backend_used = backend_used;

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

            if *stop_rx.borrow() {
                info!("🛑 Stop signal received, skipping translation emission");
                break;
            }

            info!(
                "🌐 Translation produced ({} chars)",
                translated.chars().count()
            );

            // Step 4: Emit event to frontend
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            // Pre-compute shared values to avoid redundant allocations
            let backend_str = backend_used.as_str().to_string();

            let payload = TranslationPayload {
                original: current_text.clone(),
                translated: translated.clone(),
                backend_used: backend_str.clone(),
                warnings, // Move instead of clone - not used after this
                display_state,
                timestamp,
            };

            if let Err(e) = app.emit("translation-update", payload) {
                warn!("⚠️ Failed to emit event: {}", e);
            }

            // Send subtitle update to WinUI3 OverlayHost
            // Move strings since they're no longer needed after this
            if display_state == crate::llm::TranslationDisplayState::Translated {
                let subtitle_payload = SubtitleUpdatePayload {
                    text: translated,          // Move instead of clone
                    source_text: current_text, // Move instead of clone
                    timestamp: timestamp.to_string(),
                    backend_used: Some(backend_str), // Move instead of new allocation
                };

                send_overlay_message(
                    &app,
                    IpcMessage::with_payload("Subtitle.Update", subtitle_payload),
                )
                .await;
            }

            // Wait for next iteration
            tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
        }

        let stopped_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let _ = app.emit(
            "translation-update",
            TranslationPayload {
                original: String::new(),
                translated: String::new(),
                backend_used: BackendId::Mock.as_str().to_string(),
                warnings: Vec::new(),
                display_state: TranslationDisplayState::Stopped,
                timestamp: stopped_timestamp,
            },
        );

        // Scopeguard handles cleanup: close_capture_session() and is_running reset
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

    info!("✅ Translation stopped!");
    Ok(())
}

// =============================================================================
// OVERLAY COMMANDS
// =============================================================================

/// Set whether the overlay window ignores cursor events (click-through)
///
/// When `ignore` is true, the overlay is click-through (default during translation).
/// When `ignore` is false, the overlay can receive mouse events (for settings interaction).
///
/// Called from JavaScript: `await invoke('set_overlay_click_through', { ignore: false });`
#[tauri::command]
pub fn set_overlay_click_through(app: AppHandle, ignore: bool) -> Result<(), String> {
    info!("Setting overlay click-through: {}", ignore);

    let window = app
        .get_webview_window("overlay")
        .ok_or("Overlay window not found")?;

    window
        .set_ignore_cursor_events(ignore)
        .map_err(|e| format!("Failed to set cursor events: {}", e))?;

    Ok(())
}

/// Update the overlay window region so it only contains the visible UI elements.
///
/// This is a workaround for WebView2 transparency regressions on Windows:
/// - We make the overlay window non-rectangular (border ring + subtitle box)
/// - The capture region area becomes a "hole" (not part of the window), so the
///   underlying desktop/video remains visible even if the webview background is opaque.
///
/// Called from JavaScript (overlay window):
/// `invoke('set_overlay_window_clip', { frameRegion, subtitleBounds, handleBounds, scaleFactor })`
#[tauri::command]
pub fn set_overlay_window_clip(
    app: AppHandle,
    frame_region: Option<CaptureRegion>,
    subtitle_bounds: Option<CaptureRegion>,
    handle_bounds: Option<Vec<CaptureRegion>>,
    scale_factor: f64,
) -> Result<(), String> {
    let window = app
        .get_webview_window("overlay")
        .ok_or("Overlay window not found")?;

    #[cfg(windows)]
    {
        use raw_window_handle::HasWindowHandle;
        use windows::Win32::Foundation::HWND;
        use windows::Win32::Graphics::Gdi::{
            CombineRgn, CreateRectRgn, CreateRoundRectRgn, SetWindowRgn, RGN_DIFF, RGN_OR,
        };

        // The frontend sends coordinates in CSS pixels (logical units).
        // Win32 regions operate in device pixels, so we convert using the window scale factor.
        //
        // NOTE: This does not perfectly handle multi-monitor setups where different monitors have
        // different DPI scales, but it fixes the common single-monitor case.
        let scale_factor = if scale_factor > 0.0 {
            scale_factor
        } else {
            1.0
        };

        let handle = window
            .window_handle()
            .map_err(|e| format!("Failed to get window handle: {}", e))?;

        let raw_handle = handle.as_raw();
        let hwnd = match raw_handle {
            raw_window_handle::RawWindowHandle::Win32(win32) => HWND(win32.hwnd.get() as *mut _),
            _ => return Err("Overlay window is not a Win32 window".to_string()),
        };

        // Build the region we want the overlay to occupy (in device pixels).
        // The frontend provides CSS pixel coordinates + a DPI scale factor; we convert before creating regions.
        let mut region_to_set = None;

        unsafe {
            // 1) Frame ring region (outer rect minus inner rect)
            if let Some(region) = frame_region {
                let region = region.scaled(scale_factor);
                let border_px = (2.0 * scale_factor).round().max(1.0) as i32;
                let radius_px = (8.0 * scale_factor).round().max(0.0) as i32;
                // Padding to include resize handles (positioned at -7px) and settings button (-22px).
                // The settings button is 18px tall at top: -22px, so it spans from y-22 to y-4.
                // We use 26px padding to include it with some buffer.
                let outer_padding = (26.0 * scale_factor).round() as i32;

                // Expand outer boundary to include handles and settings button
                let x1 = region.x - outer_padding;
                let y1 = region.y - outer_padding;
                let x2 = region.x + region.width + outer_padding;
                let y2 = region.y + region.height + outer_padding;

                // Outer rounded rectangle (expanded to include handles).
                let outer = CreateRoundRectRgn(x1, y1, x2, y2, radius_px * 2, radius_px * 2);
                if outer.is_invalid() {
                    return Err("CreateRoundRectRgn (outer) failed".to_string());
                }

                // Inner rounded rectangle to subtract (creates a ring).
                // The inner "hole" stays at the original region bounds (not expanded).
                let inner_x1 = region.x + border_px;
                let inner_y1 = region.y + border_px;
                let inner_x2 = region.x + region.width - border_px;
                let inner_y2 = region.y + region.height - border_px;

                if inner_x2 > inner_x1 && inner_y2 > inner_y1 {
                    let inner_radius = (radius_px - border_px).max(0);
                    let inner = CreateRoundRectRgn(
                        inner_x1,
                        inner_y1,
                        inner_x2,
                        inner_y2,
                        inner_radius * 2,
                        inner_radius * 2,
                    );
                    if inner.is_invalid() {
                        // Fallback to a rectangular inner region.
                        let inner = CreateRectRgn(inner_x1, inner_y1, inner_x2, inner_y2);
                        if inner.is_invalid() {
                            return Err("CreateRectRgn (inner fallback) failed".to_string());
                        }
                        let _ = CombineRgn(Some(outer), Some(outer), Some(inner), RGN_DIFF);
                        let _ = windows::Win32::Graphics::Gdi::DeleteObject(inner.into());
                    } else {
                        let _ = CombineRgn(Some(outer), Some(outer), Some(inner), RGN_DIFF);
                        let _ = windows::Win32::Graphics::Gdi::DeleteObject(inner.into());
                    }
                }

                region_to_set = Some(outer);
            }

            // 2) Subtitle bounds region (union)
            if let Some(bounds) = subtitle_bounds {
                let bounds = bounds.scaled(scale_factor);
                let subtitle_radius_px = (8.0 * scale_factor).round().max(0.0) as i32;
                let rgn = CreateRoundRectRgn(
                    bounds.x,
                    bounds.y,
                    bounds.x + bounds.width,
                    bounds.y + bounds.height,
                    subtitle_radius_px * 2,
                    subtitle_radius_px * 2,
                );
                if rgn.is_invalid() {
                    return Err("CreateRoundRectRgn (subtitle) failed".to_string());
                }

                match region_to_set {
                    Some(existing) => {
                        let _ = CombineRgn(Some(existing), Some(existing), Some(rgn), RGN_OR);
                        let _ = windows::Win32::Graphics::Gdi::DeleteObject(rgn.into());
                    }
                    None => {
                        region_to_set = Some(rgn);
                    }
                }
            }

            // 3) Resize handle regions (union)
            //
            // The resize handles are positioned with negative offsets in CSS, so they can extend
            // outside the capture region's bounding box. If we don't include them, they get clipped
            // by the non-rectangular window region and resizing becomes impossible.
            if let Some(handles) = handle_bounds {
                for handle in handles {
                    let handle = handle.scaled(scale_factor);

                    let right = handle.x + handle.width;
                    let bottom = handle.y + handle.height;
                    if right <= handle.x || bottom <= handle.y {
                        continue;
                    }

                    let rgn = CreateRectRgn(handle.x, handle.y, right, bottom);
                    if rgn.is_invalid() {
                        continue;
                    }

                    match region_to_set {
                        Some(existing) => {
                            let _ = CombineRgn(Some(existing), Some(existing), Some(rgn), RGN_OR);
                            let _ = windows::Win32::Graphics::Gdi::DeleteObject(rgn.into());
                        }
                        None => {
                            region_to_set = Some(rgn);
                        }
                    }
                }
            }

            // If nothing is visible, clear the region (restore rectangular window).
            // Passing None removes the region.
            match region_to_set {
                Some(rgn) => {
                    SetWindowRgn(hwnd, Some(rgn), true);
                    // DO NOT delete `rgn` after SetWindowRgn succeeds; the system owns it now.
                }
                None => {
                    SetWindowRgn(hwnd, None, true);
                }
            }
        }
    }

    Ok(())
}

// =============================================================================
// FOUNDRY SETUP WIZARD COMMANDS
// =============================================================================

/// Model information returned by the wizard
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WizardModelInfo {
    pub id: String,
    pub recommended: bool,
    pub hardware_tag: Option<String>,
}

/// Hardware information for model recommendations
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WizardHardwareInfo {
    pub arch: String,
    pub is_arm64: bool,
    pub has_npu: bool,
    pub has_gpu: bool,
    pub gpu_name: String,
    pub recommendation: String,
}

/// Disk space information
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WizardDiskSpace {
    pub available_bytes: u64,
    pub available_display: String,
}

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

/// Check whether winget is available on this system
#[tauri::command]
pub async fn wizard_check_winget() -> Result<bool, String> {
    async_runtime::spawn_blocking(|| {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            std::process::Command::new("winget")
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .creation_flags(CREATE_NO_WINDOW)
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        }
        #[cfg(not(windows))]
        {
            false
        }
    })
    .await
    .map_err(|e| e.to_string())
}

/// Spawn a visible PowerShell window that runs `winget install Microsoft.FoundryLocal`
#[tauri::command]
pub async fn wizard_install_foundry() -> Result<(), String> {
    async_runtime::spawn_blocking(|| {
        let mut cmd = std::process::Command::new("powershell");
        cmd.args([
            "-NoProfile",
            "-Command",
            "Write-Host 'Installing Foundry Local via winget...' -ForegroundColor Cyan; \
             Write-Host ''; \
             winget install Microsoft.FoundryLocal --accept-source-agreements --accept-package-agreements; \
             Write-Host ''; \
             Write-Host 'Done! You can close this window.' -ForegroundColor Green; \
             Start-Sleep -Seconds 5",
        ]);
        // Do NOT set CREATE_NO_WINDOW -- we want the user to see the installer progress
        cmd.spawn()
            .map_err(|e| format!("Failed to launch installer: {}", e))?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Poll whether Foundry CLI is now installed
#[tauri::command]
pub async fn wizard_poll_foundry_installed() -> Result<bool, String> {
    async_runtime::spawn_blocking(FoundryLocalBackend::is_cli_available)
        .await
        .map_err(|e| e.to_string())
}

/// List available models from Foundry cache for the wizard
#[tauri::command]
pub async fn wizard_list_available_models() -> Result<Vec<WizardModelInfo>, String> {
    async_runtime::spawn_blocking(|| {
        let models = FoundryLocalBackend::get_cached_models_from_cli();

        let is_arm64 = cfg!(target_arch = "aarch64");
        let is_npu = is_arm64 && cfg!(target_os = "windows");

        // Use the existing auto-model selection heuristic to find the recommended model
        let auto_pick = FoundryLocalBackend::choose_auto_model(&models);

        models
            .into_iter()
            .map(|id| {
                let recommended = auto_pick.as_deref() == Some(id.as_str());
                let hardware_tag = if is_npu && id.to_lowercase().contains("npu") {
                    Some("NPU".to_string())
                } else {
                    None
                };
                WizardModelInfo {
                    id,
                    recommended,
                    hardware_tag,
                }
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

/// Download a model using `foundry cache download`, streaming output as events
#[tauri::command]
pub async fn wizard_download_model(app: AppHandle, model_id: String) -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command as TokioCommand;

    // C1: Validate model_id to prevent injection of unexpected CLI arguments
    if model_id.is_empty()
        || model_id.len() > 200
        || model_id.contains(|c: char| !c.is_alphanumeric() && !"-._/:".contains(c))
    {
        return Err(format!("Invalid model ID: '{}'", model_id));
    }

    info!("Wizard: downloading model '{}'", model_id);

    let mut child = TokioCommand::new("foundry")
        .args(["cache", "download", &model_id])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start model download: {}", e))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Stream stdout lines as targeted events to the wizard window only
    if let Some(stdout) = stdout {
        let app_out = app.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app_out.emit_to(
                    "foundry-wizard",
                    "wizard-output",
                    serde_json::json!({"stream": "stdout", "line": line}),
                );
            }
        });
    }

    // Stream stderr lines as targeted events to the wizard window only
    if let Some(stderr) = stderr {
        let app_err = app.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app_err.emit_to(
                    "foundry-wizard",
                    "wizard-output",
                    serde_json::json!({"stream": "stderr", "line": line}),
                );
            }
        });
    }

    // Wait for process to complete
    let status = child
        .wait()
        .await
        .map_err(|e| format!("Model download failed: {}", e))?;

    if status.success() {
        info!("Wizard: model '{}' downloaded successfully", model_id);
        let _ = app.emit_to(
            "foundry-wizard",
            "wizard-download-complete",
            serde_json::json!({"success": true, "model": model_id}),
        );
    } else {
        let msg = format!("Model download exited with code: {:?}", status.code());
        warn!("Wizard: {}", msg);
        let _ = app.emit_to(
            "foundry-wizard",
            "wizard-download-complete",
            serde_json::json!({"success": false, "error": msg}),
        );
    }
    // Always return Ok -- success/failure communicated via events to avoid dual error reporting
    Ok(())
}

/// Start the Foundry service and return the service URL
#[tauri::command]
pub async fn wizard_start_service() -> Result<String, String> {
    async_runtime::spawn_blocking(|| {
        let config = crate::config::FoundryLocalConfig::default();
        let backend = FoundryLocalBackend::new(config);
        backend.ensure_service_running();
        FoundryLocalBackend::get_service_url_from_cli()
            .ok_or_else(|| "Service started but URL not found".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Get available disk space for a given path
#[tauri::command]
pub fn wizard_get_disk_space(path: String) -> Result<WizardDiskSpace, String> {
    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

        // Validate: require absolute local drive path (e.g. C:\...)
        // Reject UNC paths, drive-relative paths (C:foo), and non-drive paths
        if path.starts_with("\\\\") {
            return Err("UNC paths are not supported, only local drives".to_string());
        }
        let bytes = path.as_bytes();
        let valid_drive = bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/');
        if !valid_drive {
            return Err("Only absolute local drive paths (e.g. C:\\) are supported".to_string());
        }

        let wide_path: Vec<u16> = OsStr::new(&path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut free_bytes_available: u64 = 0;

        unsafe {
            GetDiskFreeSpaceExW(
                PCWSTR(wide_path.as_ptr()),
                Some(&mut free_bytes_available as *mut u64),
                None,
                None,
            )
            .map_err(|e| format!("GetDiskFreeSpaceExW failed: {}", e))?;
        }

        let gb = free_bytes_available as f64 / (1024.0 * 1024.0 * 1024.0);
        let display = if gb >= 1.0 {
            format!("{:.1} GB", gb)
        } else {
            let mb = free_bytes_available as f64 / (1024.0 * 1024.0);
            format!("{:.0} MB", mb)
        };

        Ok(WizardDiskSpace {
            available_bytes: free_bytes_available,
            available_display: display,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("Disk space detection is only available on Windows".to_string())
    }
}

/// Get hardware information for model recommendations
#[tauri::command]
pub fn wizard_get_hardware_info() -> WizardHardwareInfo {
    let is_arm64 = cfg!(target_arch = "aarch64");
    let has_npu = is_arm64 && cfg!(target_os = "windows");

    // Detect GPU by querying WMIC (works on all Windows editions)
    let (has_gpu, gpu_name) = {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            match std::process::Command::new("wmic")
                .args(["path", "win32_VideoController", "get", "name"])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
            {
                Ok(output) => {
                    let text = String::from_utf8_lossy(&output.stdout);
                    // Skip header line ("Name") and blank lines
                    let gpu = text
                        .lines()
                        .find(|l| {
                            let trimmed = l.trim();
                            !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("name")
                        })
                        .map(|l| l.trim().to_string());
                    let detected = gpu.is_some();
                    (detected, gpu.unwrap_or_default())
                }
                Err(_) => (false, String::new()),
            }
        }
        #[cfg(not(windows))]
        {
            (false, String::new())
        }
    };

    let recommendation = if has_npu {
        "npu".to_string()
    } else if has_gpu {
        "gpu".to_string()
    } else {
        "cpu".to_string()
    };

    WizardHardwareInfo {
        arch: std::env::consts::ARCH.to_string(),
        is_arm64,
        has_npu,
        has_gpu,
        gpu_name,
        recommendation,
    }
}
