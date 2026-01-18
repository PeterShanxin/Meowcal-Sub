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

use crate::config::{save_config, AppConfig, CaptureRegion};
use crate::capture;
use crate::ocr::WindowsOcr;
use crate::llm::{
    BackendInfo, FoundryLocalBackend, OfflineMtBackend, PhiSilica, TranslationDiagnostics,
    TranslationDiagnosticsState, TranslationManager, TranslationOutcome, TranslatorBackend,
    WindowsAiDiagnostics,
};
use crate::overlay;
use reqwest::Client;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tokio::fs;
use tokio::sync::watch;
use tracing::{info, warn, debug};

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

/// Offline MT binary detection result
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineMtDetection {
    pub path: String,
    pub source: String,
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
        info.os, info.arch, info.is_copilot_plus, 
        info.phi_silica_available, info.windows_ocr_available
    );
    
    info
}

// =============================================================================
// DOWNLOAD COMMANDS (OFFLINE MT)
// =============================================================================

const TRANSLATE_LOCALLY_BASE_URL: &str =
    "https://github.com/XapaJIaMnu/translateLocally/releases/download/latest";

/// Open the translateLocally download page in the default browser.
#[tauri::command]
pub fn open_translate_locally_download(app: AppHandle) -> Result<(), String> {
    let url = "https://github.com/XapaJIaMnu/translateLocally/releases/tag/latest";
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// Get recommended translateLocally download options for this machine.
#[tauri::command]
pub fn get_translate_locally_download_info(app: AppHandle) -> Result<TranslateLocallyDownloadInfo, String> {
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

/// Try to detect translateLocally binary on disk.
#[tauri::command]
pub fn detect_offline_mt_binary(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Option<OfflineMtDetection> {
    let config = state.config.lock().unwrap().translation.offline_mt.clone();
    OfflineMtBackend::detect_binary(&app, &config).map(|(path, source)| OfflineMtDetection {
        path: path.to_string_lossy().to_string(),
        source: source.to_string(),
    })
}

/// Get detailed diagnostics for Windows AI backend.
#[tauri::command]
pub fn get_windows_ai_diagnostics() -> WindowsAiDiagnostics {
    let phi = PhiSilica::new();
    phi.diagnostics()
}

// =============================================================================
// FOUNDRY LOCAL COMMANDS
// =============================================================================

/// Foundry Local service status
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundryLocalStatus {
    pub service_running: bool,
    pub service_url: Option<String>,
    pub models: Vec<String>,
    pub notes: String,
}

/// Get the status of Foundry Local service
#[tauri::command]
pub fn get_foundry_local_status(state: State<'_, AppState>) -> FoundryLocalStatus {
    let config = {
        let guard = state.config.lock().unwrap();
        guard.translation.foundry_local.clone()
    };

    let backend = FoundryLocalBackend::new(config);
    backend.refresh_service_status();
    let service_url = FoundryLocalBackend::get_service_url_from_cli();
    let service_running = service_url.is_some();

    // Get cached models from CLI (synchronous)
    let models = if service_running {
        FoundryLocalBackend::get_cached_models_from_cli()
    } else {
        Vec::new()
    };

    FoundryLocalStatus {
        service_running,
        service_url,
        models,
        notes: backend.notes(),
    }
}

/// List available models from Foundry Local
#[tauri::command]
pub async fn list_foundry_local_models(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let config = {
        let guard = state.config.lock().unwrap();
        guard.translation.foundry_local.clone()
    };

    let backend = FoundryLocalBackend::new(config);
    backend.refresh_service_status();
    backend.list_models().await.map_err(|e| e.to_string())
}

/// Refresh Foundry Local service status (re-detect service)
#[tauri::command]
pub fn refresh_foundry_local_status(state: State<'_, AppState>) -> FoundryLocalStatus {
    get_foundry_local_status(state)
}

/// Prepare Foundry Local (attempt to start service and refresh status)
#[tauri::command]
pub fn prepare_foundry_local(state: State<'_, AppState>) -> FoundryLocalStatus {
    let config = {
        let guard = state.config.lock().unwrap();
        guard.translation.foundry_local.clone()
    };

    let backend = FoundryLocalBackend::new(config);
    backend.refresh_service_status();

    let service_url = FoundryLocalBackend::get_service_url_from_cli();
    let service_running = service_url.is_some();
    let models = if service_running {
        FoundryLocalBackend::get_cached_models_from_cli()
    } else {
        Vec::new()
    };

    FoundryLocalStatus {
        service_running,
        service_url,
        models,
        notes: backend.notes(),
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
        default_install_dir: default_translate_locally_dir(app).to_string_lossy().to_string(),
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

fn build_option(id: &str, label: &str, asset_name: &str, notes: &str) -> TranslateLocallyDownloadOption {
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
    let config = state.config.lock().unwrap();
    config.clone()
}

/// Save new app settings
/// 
/// Called from JavaScript: `await invoke('save_settings', { settings: { ... } });`
#[tauri::command]
pub fn save_settings(
    settings: AppConfig,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    info!("Saving settings...");

    let mut updated = settings.clone();

    // Persist the last capture region into the config
    let last_region = state.capture_region.lock().unwrap().clone();
    updated.last_capture_region = last_region;

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
    let mut config = state.config.lock().unwrap();
    *config = updated.clone();

    // Persist to disk
    save_config(&app, &updated)?;

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
    info!("Setting capture region: ({}, {}) {}x{}", x, y, width, height);
    
    // Validate the region
    if width <= 0 || height <= 0 {
        return Err("Width and height must be positive".to_string());
    }
    if scale_factor <= 0.0 {
        return Err("Scale factor must be positive".to_string());
    }
    
    let region = CaptureRegion { x, y, width, height };
    
    let mut capture_region = state.capture_region.lock().unwrap();
    *capture_region = Some(region);

    let mut capture_scale_factor = state.capture_scale_factor.lock().unwrap();
    *capture_scale_factor = scale_factor;
    
    Ok(())
}

/// Get the current capture region (if set)
/// 
/// Called from JavaScript: `const region = await invoke('get_capture_region');`
#[tauri::command]
pub fn get_capture_region(state: State<'_, AppState>) -> Option<CaptureRegion> {
    let region = state.capture_region.lock().unwrap();
    region.clone()
}

/// Open the area selector overlay window
/// 
/// Called from JavaScript: `await invoke('open_area_selector');`
#[tauri::command]
pub async fn open_area_selector(app: AppHandle) -> Result<(), String> {
    info!("Opening area selector...");
    
    if let Some(window) = app.get_webview_window("selector") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        info!("✅ Area selector opened!");
    } else {
        return Err("Selector window not found".to_string());
    }
    
    Ok(())
}

/// Close the area selector overlay window
/// 
/// Called from JavaScript: `await invoke('close_area_selector');`
#[tauri::command]
pub async fn close_area_selector(app: AppHandle) -> Result<(), String> {
    info!("Closing area selector...");
    
    if let Some(window) = app.get_webview_window("selector") {
        window.hide().map_err(|e| e.to_string())?;
        info!("✅ Area selector closed!");
    } else {
        return Err("Selector window not found".to_string());
    }
    
    Ok(())
}

// =============================================================================
// TRANSLATION COMMANDS
// =============================================================================

/// Payload sent to the frontend with translation results
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationPayload {
    /// The original text from OCR
    pub original: String,
    /// The translated text
    pub translated: String,
    /// Which backend produced the translation
    pub backend_used: String,
    /// Warnings from backend selection/fallback
    pub warnings: Vec<String>,
    /// Unix timestamp in milliseconds
    pub timestamp: u64,
}

/// Payload sent to the frontend to report capture status
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureStatusPayload {
    /// Whether the capture is using the fallback method (GDI)
    pub using_fallback: bool,
    /// Human-readable message about capture status
    pub message: String,
    /// Is this an error state?
    pub is_error: bool,
}

/// List translation backends and their readiness
#[tauri::command]
pub fn list_translation_backends(state: State<'_, AppState>, app: AppHandle) -> Vec<BackendInfo> {
    let config = {
        let guard = state.config.lock().unwrap();
        guard.translation.clone()
    };

    let diagnostics = state.translation_diagnostics.clone();
    let manager = TranslationManager::new(config, app, diagnostics);
    manager.list_backends()
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
        let guard = state.config.lock().unwrap();
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
pub fn get_translation_diagnostics(
    state: State<'_, AppState>,
    app: AppHandle,
) -> TranslationDiagnostics {
    let config = {
        let guard = state.config.lock().unwrap();
        guard.translation.clone()
    };

    let diagnostics = state.translation_diagnostics.clone();
    let manager = TranslationManager::new(config, app, diagnostics);
    manager.diagnostics_snapshot()
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
pub async fn start_translation(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    info!(">>> START_TRANSLATION COMMAND CALLED <<<");
    info!("Starting translation...");
    
    // Check if already running
    {
        let is_running = state.is_running.lock().unwrap();
        if *is_running {
            return Err("Translation is already running".to_string());
        }
    }
    
    // Get the capture region
    let region = {
        let region_guard = state.capture_region.lock().unwrap();
        match region_guard.clone() {
            Some(r) => r,
            None => return Err("No capture region set. Please select an area first.".to_string()),
        }
    };

    // Get the capture scale factor (logical -> physical pixels)
    let scale_factor = {
        let scale_guard = state.capture_scale_factor.lock().unwrap();
        *scale_guard
    };
    let capture_region = region.scaled(scale_factor);
    debug!(
        "Capture scale factor: {}, logical: {:?}, physical: {:?}",
        scale_factor, region, capture_region
    );
    
    // Get settings from config
    let (interval_ms, source_language, target_language, translation_config) = {
        let config = state.config.lock().unwrap();
        (
            config.capture_interval_ms,
            config.source_language.clone(),
            config.target_language.clone(),
            config.translation.clone(),
        )
    };
    
    // Create a stop signal channel
    // The sender stays here, the receiver goes to the spawned task
    let (stop_tx, mut stop_rx) = watch::channel(false);
    
    // Store the sender so stop_translation can use it
    {
        let mut stop_signal = state.stop_signal.lock().unwrap();
        *stop_signal = Some(stop_tx);
    }
    
    // Mark as running
    {
        let mut is_running = state.is_running.lock().unwrap();
        *is_running = true;
    }
    
    info!("✅ Translation started! Interval: {}ms, Target: {}", interval_ms, target_language);
    
    // Show the overlay and send the capture region to it
    if let Err(e) = overlay::show_overlay(&app) {
        warn!("⚠️ Failed to show overlay: {}", e);
    }
    if let Err(e) = overlay::update_overlay_region(&app, &region) {
        warn!("⚠️ Failed to update overlay region: {}", e);
    }

    // Initialize translation backend manager
    let diagnostics = state.translation_diagnostics.clone();
    let translation_manager = TranslationManager::new(translation_config, app.clone(), diagnostics);
    
    // Spawn the background translation loop
    tokio::spawn(async move {
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
                match WindowsOcr::new() {
                    Ok(o) => o,
                    Err(e) => {
                        warn!("❌ Failed to initialize OCR: {}", e);
                        return;
                    }
                }
            }
        };
        
        // Keep track of last OCR text to avoid duplicate processing
        let mut last_text = String::new();
        
        // Track if we've already notified about fallback
        let mut fallback_notified = false;
        
        // Initialize persistent capture session (no border flashing)
        let session_initialized = match capture::init_capture_session() {
            Ok(_) => {
                info!("✅ Persistent capture session initialized");
                true
            }
            Err(e) => {
                warn!("⚠️ Failed to init persistent session, will use per-frame capture: {}", e);
                false
            }
        };
        
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
            
            debug!("📸 Capturing region: {:?}", capture_region);
            
            // Step 1: Capture screen region
            // If persistent session is available, use it (no border flashing)
            // Otherwise fall back to smart_capture which creates new session each time
            let capture_result = if session_initialized {
                match capture::capture_with_session(&capture_region) {
                    Ok(result) => result,
                    Err(e) => {
                        warn!("⚠️ Session capture failed: {}", e);
                        let status = CaptureStatusPayload {
                            using_fallback: false,
                            message: format!("Capture failed: {}", e),
                            is_error: true,
                        };
                        let _ = app.emit("capture-status", status);
                        tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
                        continue;
                    }
                }
            } else {
                // Fallback to smart_capture (creates new session each time)
                match capture::smart_capture(&capture_region) {
                    Ok((result, fallback)) => {
                        if fallback && !fallback_notified {
                            fallback_notified = true;
                            let status = CaptureStatusPayload {
                                using_fallback: true,
                                message: "Using GDI fallback - video content may not capture correctly".to_string(),
                                is_error: false,
                            };
                            let _ = app.emit("capture-status", status);
                        }
                        result
                    }
                    Err(e) => {
                        warn!("⚠️ Capture failed: {}", e);
                        let status = CaptureStatusPayload {
                            using_fallback: false,
                            message: format!("Capture failed: {}", e),
                            is_error: true,
                        };
                        let _ = app.emit("capture-status", status);
                        tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
                        continue;
                    }
                }
            };
            
            // Step 2: Run OCR
            let ocr_result = match ocr.recognize(
                &capture_result.data,
                capture_result.width,
                capture_result.height,
            ).await {
                Ok(result) => result,
                Err(e) => {
                    warn!("⚠️ OCR failed: {}", e);
                    tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
                    continue;
                }
            };
            
            // Skip if empty or same as last frame
            if ocr_result.is_empty() {
                debug!("No text detected, skipping");
                tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
                continue;
            }
            
            let current_text = ocr_result.text.trim().to_string();
            if current_text == last_text {
                debug!("Same text as last frame, skipping");
                tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
                continue;
            }
            
            last_text = current_text.clone();
            info!("📝 OCR detected ({} chars)", current_text.chars().count());
            
            // Step 3: Translate via backend manager with fallback
            let outcome = translation_manager
                .translate_with_fallback(&current_text, &source_language, &target_language)
                .await;
            let TranslationOutcome {
                translated,
                backend_used,
                warnings,
            } = outcome;
            
            info!("🌐 Translation produced ({} chars)", translated.chars().count());
            
            // Step 4: Emit event to frontend
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            
            let payload = TranslationPayload {
                original: current_text,
                translated,
                backend_used: backend_used.as_str().to_string(),
                warnings,
                timestamp,
            };
            
            if let Err(e) = app.emit("translation-update", payload) {
                warn!("⚠️ Failed to emit event: {}", e);
            }
            
            // Wait for next iteration
            tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
        }
        
        // Close the capture session when loop ends
        capture::close_capture_session();
        info!("Translation loop ended");
    });
    
    Ok(())
}

/// Stop the translation process
/// 
/// Called from JavaScript: `await invoke('stop_translation');`
#[tauri::command]
pub fn stop_translation(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    info!("Stopping translation...");
    
    // Send the stop signal
    {
        let stop_signal = state.stop_signal.lock().unwrap();
        if let Some(ref sender) = *stop_signal {
            let _ = sender.send(true);
            info!("Stop signal sent");
        }
    }
    
    // Clear the stop signal sender
    {
        let mut stop_signal = state.stop_signal.lock().unwrap();
        *stop_signal = None;
    }
    
    // Mark as not running
    {
        let mut is_running = state.is_running.lock().unwrap();
        *is_running = false;
    }
    
    // Close the capture session
    capture::close_capture_session();
    
    // Hide the overlay
    if let Err(e) = overlay::hide_overlay(&app) {
        warn!("⚠️ Failed to hide overlay: {}", e);
    }
    
    info!("✅ Translation stopped!");
    Ok(())
}
