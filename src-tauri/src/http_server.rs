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
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

use crate::config::{AppConfig, CaptureRegion};
use crate::llm::{
    BackendId, BackendInfo, FoundryLocalBackend, FoundryLocalPhase, OfflineMtBackend, PhiSilica,
    ReadyState, TranslationDiagnostics, TranslationDiagnosticsState, TranslatorBackend,
};

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
    pub service_running: bool,
    pub service_url: Option<String>,
    pub models: Vec<String>,
    pub notes: String,
    /// Granular Foundry Local phase (e.g. notInstalled, notRunning, noModels, preparing, ready).
    pub phase: FoundryLocalPhase,
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
    let config = state.config.lock().unwrap().clone();
    Json(config)
}

/// POST /api/settings - Save settings
async fn save_settings(
    State(state): State<HttpAppState>,
    Json(settings): Json<AppConfig>,
) -> impl IntoResponse {
    // Update in-memory config
    {
        let mut config = state.config.lock().unwrap();
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
    let config = state.config.lock().unwrap().clone();

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

    let diagnostics_state = state.translation_diagnostics.lock().unwrap();
    let (last_error_by_backend, last_latency_by_backend) = diagnostics_state.snapshot();

    Json(TranslationDiagnostics {
        backends,
        last_error_by_backend,
        last_latency_by_backend,
    })
}

/// GET /api/foundry-local/models - List Foundry Local models
async fn list_foundry_local_models(State(state): State<HttpAppState>) -> impl IntoResponse {
    let config = state.config.lock().unwrap().clone();
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
    let config = state.config.lock().unwrap().clone();
    let backend = FoundryLocalBackend::new(config.translation.foundry_local);
    backend.refresh_service_status();

    let service_url = FoundryLocalBackend::get_service_url_from_cli();
    let service_running = service_url.is_some();
    let models = if service_running {
        FoundryLocalBackend::get_cached_models_from_cli()
    } else {
        Vec::new()
    };

    // In browser mode, probe isn't practical (no async blocking context),
    // so return "preparing" when service is running with models
    let phase = if !FoundryLocalBackend::is_cli_available() {
        FoundryLocalPhase::NotInstalled
    } else if !service_running {
        FoundryLocalPhase::NotRunning
    } else if models.is_empty() {
        FoundryLocalPhase::NoModels
    } else {
        // Can't probe in browser mode (and we don't want to warm models implicitly).
        FoundryLocalPhase::Unchecked
    };

    Json(FoundryLocalStatus {
        service_running,
        service_url,
        models,
        notes: backend.notes(),
        phase,
    })
}

/// POST /api/foundry-local/prepare - Attempt to start Foundry Local service
async fn prepare_foundry_local(State(state): State<HttpAppState>) -> impl IntoResponse {
    let config = state.config.lock().unwrap().clone();
    let backend = FoundryLocalBackend::new(config.translation.foundry_local);

    backend.ensure_service_running();

    let service_url = FoundryLocalBackend::get_service_url_from_cli();
    let service_running = service_url.is_some();
    let models = if service_running {
        FoundryLocalBackend::get_cached_models_from_cli()
    } else {
        Vec::new()
    };

    // In browser mode, probe isn't practical, so return "preparing" when service is running with models
    let phase = if !FoundryLocalBackend::is_cli_available() {
        FoundryLocalPhase::NotInstalled
    } else if !service_running {
        FoundryLocalPhase::NotRunning
    } else if models.is_empty() {
        FoundryLocalPhase::NoModels
    } else {
        // Can't probe in browser mode (and we don't want to warm models implicitly).
        FoundryLocalPhase::Unchecked
    };

    Json(FoundryLocalStatus {
        service_running,
        service_url,
        models,
        notes: backend.notes(),
        phase,
    })
}

/// GET /api/windows-ai/diagnostics - Get Windows AI diagnostics
async fn get_windows_ai_diagnostics() -> impl IntoResponse {
    let phi = PhiSilica::new();
    Json(phi.diagnostics())
}

/// GET /api/offline-mt/detect - Detect Offline MT binary
async fn detect_offline_mt_binary(State(state): State<HttpAppState>) -> impl IntoResponse {
    let config = state.config.lock().unwrap().clone();

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
    let region = *state.capture_region.lock().unwrap();
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
    *state.capture_region.lock().unwrap() = Some(region);
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
        .route("/api/foundry-local/prepare", post(prepare_foundry_local))
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
