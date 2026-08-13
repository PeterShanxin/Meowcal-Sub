// HTTP API for browser dev mode. Engine status/refresh/prepare/make-ready
// orchestration lives in engine_status; this file is the thin adapter.

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
use crate::engine_status::{self, EngineStatusSnapshot};
use crate::http_config::{
    get_settings, load_standalone_config, save_settings, standalone_config_path,
};
use crate::llm::{
    BackendId, BackendInfo, FoundryLocalBackend, FoundryLocalPhase, ReadyState,
    TranslationDiagnostics, TranslationDiagnosticsState, TranslatorBackend,
};
use crate::sync_utils::lock_or_recover;

#[derive(Clone)]
pub struct HttpAppState {
    pub config: Arc<Mutex<AppConfig>>,
    pub capture_region: Arc<Mutex<Option<CaptureRegion>>>,
    pub translation_diagnostics: Arc<Mutex<TranslationDiagnosticsState>>,
    pub config_path: PathBuf,
}

impl HttpAppState {
    pub fn new() -> Self {
        let config_path = standalone_config_path();
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

fn foundry_status_from_snapshot(s: EngineStatusSnapshot) -> FoundryLocalStatus {
    FoundryLocalStatus {
        cli_available: s.cli_available,
        service_running: s.service_running,
        service_url: s.service_url,
        models: s.models,
        configured_model: s.configured_model,
        selected_model: s.selected_model,
        notes: s.notes,
        phase: s.phase,
        probe: s.probe,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserModeInfo {
    browser_mode: bool,
    message: String,
}

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

/// GET /api/ocr/languages - List installed OCR language packs
async fn get_ocr_languages_http() -> impl IntoResponse {
    use crate::ocr::WindowsOcr;
    let result = tokio::task::spawn_blocking(WindowsOcr::available_languages).await;
    match result {
        Ok(Ok(langs)) => Json(langs),
        _ => Json(vec![]),
    }
}

/// GET /api/translation/diagnostics - Get translation backend diagnostics
async fn get_translation_diagnostics(State(state): State<HttpAppState>) -> impl IntoResponse {
    let config = lock_or_recover(&state.config).clone();

    // Build backend list with real status checks
    let mut backends: Vec<BackendInfo> = Vec::new();

    // Local translation engine
    if config.translation.enable_foundry_local {
        let foundry = FoundryLocalBackend::new(config.translation.foundry_local.clone());
        // Refresh to detect service URL and populate notes correctly
        foundry.refresh_service_status();
        let phase = foundry.phase();
        backends.push(BackendInfo {
            id: BackendId::FoundryLocal,
            name: "Local Translation Engine".to_string(),
            available: foundry.is_available(),
            ready_state: foundry.ready_state(),
            notes: foundry.notes(),
            phase: Some(phase),
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

/// GET /api/engine/models - List local engine models
async fn list_engine_models(State(state): State<HttpAppState>) -> impl IntoResponse {
    let config = lock_or_recover(&state.config).clone();
    let backend = FoundryLocalBackend::new(config.translation.foundry_local);

    // Refresh service status to detect service URL before listing models
    backend.refresh_service_status();

    match backend.list_models().await {
        Ok(models) => Json(serde_json::json!({ "models": models })),
        Err(e) => Json(serde_json::json!({ "models": [], "error": e.to_string() })),
    }
}

/// GET /api/engine/status - Get local engine status
async fn get_engine_status(State(state): State<HttpAppState>) -> impl IntoResponse {
    let config = lock_or_recover(&state.config).clone();
    let snapshot = engine_status::get_status_http(config.translation.foundry_local.clone()).await;
    Json(foundry_status_from_snapshot(snapshot))
}

/// POST /api/engine/refresh - Refresh engine status (fast, read-only probe)
async fn refresh_engine_status(State(state): State<HttpAppState>) -> impl IntoResponse {
    let config = lock_or_recover(&state.config).clone();
    let snapshot =
        engine_status::refresh_status_http(config.translation.foundry_local.clone()).await;
    Json(foundry_status_from_snapshot(snapshot))
}

/// POST /api/engine/prepare - Attempt to start the engine service
async fn prepare_engine(State(state): State<HttpAppState>) -> impl IntoResponse {
    let config = lock_or_recover(&state.config).clone();
    let snapshot = engine_status::prepare_http(config.translation.foundry_local.clone()).await;
    Json(foundry_status_from_snapshot(snapshot))
}

/// POST /api/engine/make-ready - Start service if needed + keep probing until Ready (or timeout)
async fn make_engine_ready(State(state): State<HttpAppState>) -> impl IntoResponse {
    let config = lock_or_recover(&state.config).clone();
    let snapshot = engine_status::make_ready_http(config.translation.foundry_local.clone()).await;
    Json(foundry_status_from_snapshot(snapshot))
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

/// OCR install - Not available in browser mode (requires OS-level access)
async fn ocr_install_not_available() -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(BrowserModeInfo {
            browser_mode: true,
            message: "OCR language pack installation requires the desktop app. Not available in browser mode."
                .to_string(),
        }),
    )
}

/// Wizard endpoints - Not available in browser mode (require Tauri window)
async fn wizard_not_available() -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(BrowserModeInfo {
            browser_mode: true,
            message: "Setup wizard requires Tauri window. Not available in browser mode."
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
        // Local translation engine
        .route("/api/engine/models", get(list_engine_models))
        .route("/api/engine/status", get(get_engine_status))
        .route("/api/engine/refresh", post(refresh_engine_status))
        .route("/api/engine/prepare", post(prepare_engine))
        .route("/api/engine/make-ready", post(make_engine_ready))
        // Capture region
        .route("/api/capture-region", get(get_capture_region))
        .route("/api/capture-region", post(set_capture_region))
        // OCR language management
        .route("/api/ocr/languages", get(get_ocr_languages_http))
        .route("/api/ocr/install-language", post(ocr_install_not_available))
        // Tauri-only endpoints (return 501)
        .route("/api/area-selector", post(open_area_selector))
        .route("/api/translation/start", post(start_translation))
        .route("/api/translation/stop", post(stop_translation))
        // Wizard endpoints (Tauri-only, return 501 in browser mode)
        .route("/api/wizard/open", post(wizard_not_available))
        .route("/api/wizard/close", post(wizard_not_available))
        .route("/api/wizard/install-engine", post(wizard_not_available))
        .route("/api/wizard/start-service", post(wizard_not_available))
        .route("/api/wizard/test-translation", post(wizard_not_available))
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
