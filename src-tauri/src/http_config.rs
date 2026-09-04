// =============================================================================
// HTTP_CONFIG.RS - the config file as the browser dev server sees it
// =============================================================================
// Browser dev mode (`--http-only`) runs without a Tauri `AppHandle`, so it
// cannot use `config_store::load_config` or `config_save::save_config` - both
// resolve their path from one. It therefore grew its own loader and saver, and
// those kept the bug #64 fixed everywhere else: an unparseable config became
// factory defaults, and a save with no staging file could truncate it.
//
// The standalone server follows the current build profile's namespace, so its
// settings remain separate from the installed application's settings.
//
// - The Tauri path re-attaches the capture region, its scale factor and the
//   window geometry from live state before saving. This path has no such state,
//   so `config_save` preserves those from disk when a save does not carry them.
// =============================================================================

use crate::app_profile::AppProfile;
use crate::config::AppConfig;
use crate::http_server::HttpAppState;
use crate::sync_utils::lock_or_recover;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Where the standalone HTTP server reads and writes settings.
pub fn standalone_config_path() -> PathBuf {
    let appdata = if cfg!(target_os = "windows") {
        std::env::var("APPDATA").ok()
    } else {
        None
    };
    standalone_config_path_for(AppProfile::current(), appdata.as_deref())
}

/// Resolve a standalone config path without reading process-global state.
fn standalone_config_path_for(profile: AppProfile, appdata: Option<&str>) -> PathBuf {
    if let Some(appdata) = appdata {
        return PathBuf::from(appdata)
            .join(profile.identifier())
            .join("config.json");
    }

    match profile {
        AppProfile::Production => PathBuf::from("config.json"),
        AppProfile::Development => PathBuf::from("config.dev.json"),
    }
}

/// Load config without Tauri's AppHandle, with the durability the app has.
///
/// Recovery, quarantine and provenance all come from `load_and_report`, so a
/// config the dev server cannot read is restored from the backup rather than
/// replaced by defaults - and the session is marked as holding a fallback, so
/// its later saves cannot overwrite a config that recovers underneath it.
///
/// This writes. The loader it replaced could not touch the disk, and this one
/// refreshes the backup on a clean load and can quarantine and restore on a bad
/// one - against the current standalone profile. It does mean this is not a
/// read-only way to inspect a profile, and a router test pointed at a real
/// `%APPDATA%` would rewrite it.
pub fn load_standalone_config(path: &Path) -> AppConfig {
    info!("Browser dev mode is using config at {}", path.display());
    crate::config_store::load_and_report(path)
}

/// Save config without Tauri's AppHandle, atomically.
///
/// `fs::write` truncates before writing, so an interrupted dev-mode save left a
/// config that parsed as nothing - the same loss the loader above used to
/// absorb. `save_to` stages and renames, refuses to overwrite a file it could
/// not read, and restores the app-owned fields this caller cannot supply: the
/// settings form knows nothing of the capture region or the window geometry, and
/// writing its payload as-is used to blank both.
pub fn save_standalone_config(path: &Path, config: &AppConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    crate::config_save::save_to(path, config, crate::config_save::session_provenance())
}

/// The same save, off the async worker that asked for it.
///
/// The save retries reads with real `thread::sleep` and ends in an `fsync`, so
/// worst case it parks a runtime thread for a quarter of a second plus the flush.
/// The Tauri path offloads the identical work for the identical reason.
pub async fn save_standalone_config_offloaded(
    path: PathBuf,
    config: AppConfig,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || save_standalone_config(&path, &config))
        .await
        .unwrap_or_else(|error| Err(format!("Failed to run the settings save: {error}")))
}

/// GET /api/settings - Get current settings
pub async fn get_settings(State(state): State<HttpAppState>) -> impl IntoResponse {
    let config = lock_or_recover(&state.config).clone();
    Json(config)
}

/// POST /api/settings - Save settings
///
/// Written first, adopted only if it lands. `save_to` can refuse - an unreadable
/// file, or a fallback session against a config that recovered - and taking the
/// settings into memory regardless would leave the server serving settings that
/// are not the ones on disk.
pub async fn save_settings(
    State(state): State<HttpAppState>,
    Json(settings): Json<AppConfig>,
) -> impl IntoResponse {
    match save_standalone_config_offloaded(state.config_path.clone(), settings.clone()).await {
        Ok(()) => {
            *lock_or_recover(&state.config) = settings;
            (StatusCode::OK, Json(serde_json::json!({ "success": true }))).into_response()
        }
        // A status the caller's error path can see. As a 200 this read as success
        // to the browser bridge, which only throws on `!response.ok`, so a
        // refused save reported "Settings saved" and the setting was gone at the
        // next launch.
        Err(error) => {
            warn!("Settings save refused: {error}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "success": false, "error": error })),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
#[path = "http_config_tests.rs"]
mod tests;
