// =============================================================================
// HTTP_SERVER.RS - HTTP API for Browser Dev Mode
// =============================================================================
// This module provides an HTTP server that exposes the same functionality
// as the Tauri IPC commands, allowing the frontend to run in a browser
// and still communicate with the real Rust backend.
//
// Used for development and testing by AI agents (Claude) who can access
// the app through a browser but not through Tauri's WebView.
// =============================================================================

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

use crate::config::{AppConfig, CaptureRegion};
use crate::llm::{
    BackendId, BackendInfo, FoundryLocalBackend, FoundryLocalPhase, OfflineMtBackend, PhiSilica,
    ReadyState, TranslationDiagnostics, TranslationDiagnosticsState, TranslatorBackend,
    FAST_PROBE_TIMEOUT_MS, SLOW_PROBE_TIMEOUT_MS,
};
use crate::sync_utils::lock_or_recover;

// =============================================================================
// HTTP SERVER STATE
// =============================================================================

/// Shared state for the HTTP server (similar to Tauri's AppState)
#[derive(Clone)]
pub struct HttpAppState {
    pub config: Arc<Mutex<AppConfig>>,
    pub capture_region: Arc<Mutex<Option<CaptureRegion>>>,
    pub translation_diagnostics: Arc<Mutex<TranslationDiagnosticsState>>,
    pub config_path: PathBuf,
}

impl HttpAppState {
    pub fn new() -> Self {
        let config_path = get_standalone_config_path();
        let config = load_standalone_config(&config_path);

        Self {
            config: Arc::new(Mutex::new(config)),
            capture_region: Arc::new(Mutex::new(None)),
            translation_diagnostics: Arc::new(Mutex::new(TranslationDiagnosticsState::default())),
            config_path,
        }
    }
}

impl Default for HttpAppState {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// STANDALONE CONFIG HELPERS
// =============================================================================

/// Get config path without Tauri's AppHandle
fn get_standalone_config_path() -> PathBuf {
    if cfg!(target_os = "windows") {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata)
                .join("com.meowcal.sub")
                .join("config.json");
        }
    }
    // Fallback
    PathBuf::from("config.json")
}

/// Load config without Tauri's AppHandle
fn load_standalone_config(path: &PathBuf) -> AppConfig {
    if let Ok(content) = std::fs::read_to_string(path) {
        if let Ok(config) = serde_json::from_str::<AppConfig>(&content) {
            return config;
        }
    }
    AppConfig::default()
}

/// Save config without Tauri's AppHandle
fn save_standalone_config(path: &PathBuf, config: &AppConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())?;
    Ok(())
}

// =============================================================================
// API RESPONSE TYPES
// =============================================================================

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub os: String,
    pub arch: String,
    pub is_copilot_plus: bool,
    pub phi_silica_available: bool,
    pub windows_ocr_available: bool,
}

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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineMtDetection {
    pub path: String,
    pub source: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserModeInfo {
    browser_mode: bool,
    message: String,
}

// =============================================================================
// API HANDLERS
// =============================================================================

/// GET /api/health - Health check
async fn health() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "browserMode": true,
        "message": "Meowcal Sub HTTP API is running"
    }))
}

/// GET /api/system-info - Get system information
async fn get_system_info() -> impl IntoResponse {
    let is_arm64 = cfg!(target_arch = "aarch64");
    let is_copilot_plus = is_arm64 && cfg!(target_os = "windows");
    let phi_silica_available = false; // TODO: detect
    let windows_ocr_available = cfg!(target_os = "windows");

    Json(SystemInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        is_copilot_plus,
        phi_silica_available,
        windows_ocr_available,
    })
}

/// GET /api/settings - Get current settings
async fn get_settings(State(state): State<HttpAppState>) -> impl IntoResponse {
    let config = lock_or_recover(&state.config).clone();
    Json(config)
}

/// POST /api/settings - Save settings
async fn save_settings(
    State(state): State<HttpAppState>,
    Json(settings): Json<AppConfig>,
) -> impl IntoResponse {
    // Update in-memory config
    {
        let mut config = lock_or_recover(&state.config);
        *config = settings.clone();
    }

    // Persist to disk
    match save_standalone_config(&state.config_path, &settings) {
        Ok(_) => Json(serde_json::json!({ "success": true })),
        Err(e) => Json(serde_json::json!({ "success": false, "error": e })),
    }
}

/// GET /api/translation/diagnostics - Get translation backend diagnostics
async fn get_translation_diagnostics(State(state): State<HttpAppState>) -> impl IntoResponse {
    let config = lock_or_recover(&state.config).clone();

    // Build backend list with real status checks
    let mut backends: Vec<BackendInfo> = Vec::new();

    // Foundry Local
    if config.translation.enable_foundry_local {
        let foundry = FoundryLocalBackend::new(config.translation.foundry_local.clone());
        // Refresh to detect service URL and populate notes correctly
        foundry.refresh_service_status();
        let phase = foundry.phase();
        backends.push(BackendInfo {
            id: BackendId::FoundryLocal,
            name: "Foundry Local".to_string(),
            available: foundry.is_available(),
            ready_state: foundry.ready_state(),
            notes: foundry.notes(),
            phase: Some(phase),
        });
    }

    // Offline MT
    if config.translation.enable_offline_mt {
        let offline = OfflineMtBackend::new_standalone(config.translation.offline_mt.clone());
        backends.push(BackendInfo {
            id: BackendId::OfflineMt,
            name: "Offline MT (translateLocally)".to_string(),
            available: true,
            ready_state: offline.ready_state(),
            notes: offline.notes(),
            phase: None,
        });
    }

    // Windows AI / Phi Silica
    if config.translation.enable_windows_ai {
        let phi = PhiSilica::new();
        backends.push(BackendInfo {
            id: BackendId::WindowsAi,
            name: "Windows AI (Phi Silica)".to_string(),
            available: true,
            ready_state: phi.ready_state(),
            notes: phi.notes(),
            phase: None,
        });
    }

    // Mock/Passthrough
    if config.translation.allow_mock_fallback {
        backends.push(BackendInfo {
            id: BackendId::Mock,
            name: "Passthrough (No Translation)".to_string(),
            available: true,
            ready_state: ReadyState::Ready,
            notes: "Returns original text without translation".to_string(),
            phase: None,
        });
    }

    let diagnostics_state = lock_or_recover(&state.translation_diagnostics);
    let (last_error_by_backend, last_latency_by_backend) = diagnostics_state.snapshot();

    Json(TranslationDiagnostics {
        backends,
        last_error_by_backend,
        last_latency_by_backend,
    })
}

/// GET /api/foundry-local/models - List Foundry Local models
async fn list_foundry_local_models(State(state): State<HttpAppState>) -> impl IntoResponse {
    let config = lock_or_recover(&state.config).clone();
    let backend = FoundryLocalBackend::new(config.translation.foundry_local);

    // Refresh service status to detect service URL before listing models
    backend.refresh_service_status();

    match backend.list_models().await {
        Ok(models) => Json(serde_json::json!({ "models": models })),
        Err(e) => Json(serde_json::json!({ "models": [], "error": e.to_string() })),
    }
}

/// GET /api/foundry-local/status - Get Foundry Local status
async fn get_foundry_local_status(State(state): State<HttpAppState>) -> impl IntoResponse {
    let config = lock_or_recover(&state.config).clone();
    let foundry_config = config.translation.foundry_local.clone();
    let configured_model = foundry_config.model.clone();
    let backend = FoundryLocalBackend::new(foundry_config);
    backend.refresh_service_status();

    let cli_available = FoundryLocalBackend::is_cli_available();
    let service_url = FoundryLocalBackend::get_service_url_from_cli();
    let service_running = service_url.is_some();
    let models = if service_running {
        FoundryLocalBackend::get_cached_models_from_cli()
    } else {
        Vec::new()
    };

    // No network probe here; this is a fast snapshot.
    let phase = backend.phase();

    Json(FoundryLocalStatus {
        cli_available,
        service_running,
        service_url,
        models,
        configured_model,
        selected_model: backend.selected_model(),
        notes: backend.notes(),
        phase,
        probe: backend.probe_snapshot(),
    })
}

/// POST /api/foundry-local/refresh - Refresh Foundry Local status (fast, read-only probe)
async fn refresh_foundry_local_status(State(state): State<HttpAppState>) -> impl IntoResponse {
    let config = lock_or_recover(&state.config).clone();
    let foundry_config = config.translation.foundry_local.clone();
    let configured_model = foundry_config.model.clone();

    let (backend, cli_available, service_url, service_running, models, notes) =
        tokio::task::spawn_blocking({
            let config = foundry_config.clone();
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
        .unwrap_or_else(|_| {
            let backend = FoundryLocalBackend::new(foundry_config.clone());
            (
                backend,
                false,
                None,
                false,
                Vec::new(),
                "Foundry Local refresh task failed".to_string(),
            )
        });

    let phase = if service_running && !models.is_empty() {
        if backend.is_probe_cache_valid() {
            FoundryLocalPhase::Ready
        } else {
            match backend.probe_chat_completions(FAST_PROBE_TIMEOUT_MS).await {
                Ok(true) => FoundryLocalPhase::Ready,
                Ok(false) => FoundryLocalPhase::Preparing,
                Err(_) => FoundryLocalPhase::Error,
            }
        }
    } else {
        backend.phase()
    };

    Json(FoundryLocalStatus {
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

/// POST /api/foundry-local/prepare - Attempt to start Foundry Local service
async fn prepare_foundry_local(State(state): State<HttpAppState>) -> impl IntoResponse {
    let config = lock_or_recover(&state.config).clone();
    let foundry_config = config.translation.foundry_local.clone();
    let configured_model = foundry_config.model.clone();
    let (backend, cli_available, service_url, service_running, models, mut notes) =
        tokio::task::spawn_blocking({
            let config = foundry_config.clone();
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
        .unwrap_or_else(|_| {
            let backend = FoundryLocalBackend::new(foundry_config.clone());
            (
                backend,
                false,
                None,
                false,
                Vec::new(),
                "Foundry Local prepare task failed".to_string(),
            )
        });

    let phase = if service_running && !models.is_empty() {
        match backend.probe_chat_completions(SLOW_PROBE_TIMEOUT_MS).await {
            Ok(true) => FoundryLocalPhase::Ready,
            Ok(false) => FoundryLocalPhase::Preparing,
            Err(e) => {
                notes = format!("{} Probe error: {}", notes, e);
                FoundryLocalPhase::Error
            }
        }
    } else {
        backend.phase()
    };

    Json(FoundryLocalStatus {
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

/// POST /api/foundry-local/make-ready - Start service if needed + keep probing until Ready (or timeout)
async fn make_foundry_ready(State(state): State<HttpAppState>) -> impl IntoResponse {
    let config = lock_or_recover(&state.config).clone();
    let foundry_config = config.translation.foundry_local.clone();
    let configured_model = foundry_config.model.clone();
    let configured_timeout_ms = foundry_config.timeout_ms as u64;
    let steady_probe_timeout_ms = configured_timeout_ms.clamp(5_000, SLOW_PROBE_TIMEOUT_MS);

    let (backend, cli_available, mut service_url, mut service_running, mut models, mut notes) =
        tokio::task::spawn_blocking({
            let config = foundry_config.clone();
            move || {
                let backend = FoundryLocalBackend::new(config);
                let cli_available = FoundryLocalBackend::is_cli_available();
                if cli_available {
                    backend.ensure_service_running();
                }
                backend.refresh_service_status();
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
        .unwrap_or_else(|_| {
            let backend = FoundryLocalBackend::new(foundry_config.clone());
            (
                backend,
                false,
                None,
                false,
                Vec::new(),
                "Foundry Local make-ready task failed".to_string(),
            )
        });

    if !cli_available || !service_running || models.is_empty() {
        let phase = backend.phase();
        return Json(FoundryLocalStatus {
            cli_available,
            service_running,
            service_url,
            models,
            configured_model,
            selected_model: backend.selected_model(),
            notes,
            phase,
            probe: backend.probe_snapshot(),
        });
    }

    let started = Instant::now();
    let max_total = Duration::from_secs(90);
    let mut attempt = 0usize;
    let mut phase = FoundryLocalPhase::Preparing;
    let mut last_error: Option<String> = None;

    while started.elapsed() < max_total {
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
            }
        }

        backend.refresh_service_status();
        service_url = FoundryLocalBackend::get_service_url_from_cli();
        service_running = service_url.is_some();
        models = if service_running {
            FoundryLocalBackend::get_cached_models_from_cli()
        } else {
            Vec::new()
        };

        if !service_running {
            phase = FoundryLocalPhase::NotRunning;
            break;
        }

        tokio::time::sleep(Duration::from_millis(1500)).await;
    }

    if phase != FoundryLocalPhase::Ready {
        if let Some(err) = last_error {
            notes = format!("{} Last error: {}", notes, err);
        } else {
            notes = format!("{} Still warming up. Try again shortly.", notes);
        }
    }

    Json(FoundryLocalStatus {
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

/// GET /api/windows-ai/diagnostics - Get Windows AI diagnostics
async fn get_windows_ai_diagnostics() -> impl IntoResponse {
    let phi = PhiSilica::new();
    Json(phi.diagnostics())
}

/// GET /api/offline-mt/detect - Detect Offline MT binary
async fn detect_offline_mt_binary(State(state): State<HttpAppState>) -> impl IntoResponse {
    let config = lock_or_recover(&state.config).clone();

    // Use standalone detection (checks PATH and common locations)
    match OfflineMtBackend::detect_binary_standalone(&config.translation.offline_mt) {
        Some((path, source)) => Json(serde_json::json!({
            "found": true,
            "path": path.to_string_lossy(),
            "source": source
        })),
        None => Json(serde_json::json!({
            "found": false,
            "path": null,
            "source": null
        })),
    }
}

/// POST /api/area-selector - Not available in browser mode
async fn open_area_selector() -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(BrowserModeInfo {
            browser_mode: true,
            message: "Area selector requires Tauri window. Not available in browser mode."
                .to_string(),
        }),
    )
}

/// POST /api/translation/start - Not available in browser mode
async fn start_translation() -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(BrowserModeInfo {
            browser_mode: true,
            message: "Screen capture requires Windows APIs. Not available in browser mode."
                .to_string(),
        }),
    )
}

/// POST /api/translation/stop - Not available in browser mode
async fn stop_translation() -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(BrowserModeInfo {
            browser_mode: true,
            message: "Translation control requires Tauri runtime. Not available in browser mode."
                .to_string(),
        }),
    )
}

/// GET /api/capture-region - Get current capture region
async fn get_capture_region(State(state): State<HttpAppState>) -> impl IntoResponse {
    let region = *lock_or_recover(&state.capture_region);
    Json(region)
}

/// POST /api/capture-region - Set capture region (for testing)
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetRegionRequest {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

async fn set_capture_region(
    State(state): State<HttpAppState>,
    Json(req): Json<SetRegionRequest>,
) -> impl IntoResponse {
    let region = CaptureRegion {
        x: req.x,
        y: req.y,
        width: req.width,
        height: req.height,
    };
    *lock_or_recover(&state.capture_region) = Some(region);
    Json(serde_json::json!({ "success": true }))
}

// =============================================================================
// SERVER SETUP
// =============================================================================

/// Create the HTTP router with all API endpoints
pub fn create_router(state: HttpAppState) -> Router {
    // CORS layer to allow browser access
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // Health check
        .route("/api/health", get(health))
        // System info
        .route("/api/system-info", get(get_system_info))
        // Settings
        .route("/api/settings", get(get_settings))
        .route("/api/settings", post(save_settings))
        // Translation diagnostics
        .route(
            "/api/translation/diagnostics",
            get(get_translation_diagnostics),
        )
        // Foundry Local
        .route("/api/foundry-local/models", get(list_foundry_local_models))
        .route("/api/foundry-local/status", get(get_foundry_local_status))
        .route(
            "/api/foundry-local/refresh",
            post(refresh_foundry_local_status),
        )
        .route("/api/foundry-local/prepare", post(prepare_foundry_local))
        .route("/api/foundry-local/make-ready", post(make_foundry_ready))
        // Windows AI
        .route(
            "/api/windows-ai/diagnostics",
            get(get_windows_ai_diagnostics),
        )
        // Offline MT
        .route("/api/offline-mt/detect", get(detect_offline_mt_binary))
        // Capture region
        .route("/api/capture-region", get(get_capture_region))
        .route("/api/capture-region", post(set_capture_region))
        // Tauri-only endpoints (return 501)
        .route("/api/area-selector", post(open_area_selector))
        .route("/api/translation/start", post(start_translation))
        .route("/api/translation/stop", post(stop_translation))
        .layer(cors)
        .with_state(state)
}

/// Start the HTTP server on the specified port
pub async fn start_server(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let state = HttpAppState::new();
    let app = create_router(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    info!("🌐 HTTP API server starting on http://{}", addr);
    info!("   Open http://localhost:3000 in your browser to test");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
