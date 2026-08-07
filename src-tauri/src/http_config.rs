// =============================================================================
// HTTP_CONFIG.RS - the config file as the browser dev server sees it
// =============================================================================
// Browser dev mode (`--http-only`) runs without a Tauri `AppHandle`, so it
// cannot use `config_store::load_config` or `config_save::save_config` - both
// resolve their path from one. It therefore grew its own loader and saver, and
// those kept the bug #64 fixed everywhere else: an unparseable config became
// factory defaults, and a save with no staging file could truncate it.
//
// That was never a dev-only risk. `%APPDATA%\com.meowcal.sub\config.json` is
// the installed app's config too (#68), so `npm run dev:backend` against a real
// profile could reproduce the original incident in full - engine registration
// and capture region gone. See issue #71.
//
// Storage is now shared, reached through the path-taking entry points. What is
// not shared is the surrounding behaviour, and two differences are worth naming
// rather than discovering:
//
// - The Tauri path re-attaches the capture region, its scale factor and the
//   window geometry from live state before saving. This path has no such state,
//   so `config_save` preserves those from disk when a save does not carry them.
// - Both processes can be running against the one file (#68), and neither
//   revalidates between reading and writing. A save from here still writes the
//   settings the browser was shown, so a change the app made in between is lost.
//   Separating the profiles is the fix; until then this door is no safer to use
//   concurrently than it ever was.
// =============================================================================

use crate::config::AppConfig;
use crate::http_server::HttpAppState;
use crate::sync_utils::lock_or_recover;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Where the dev server reads and writes settings.
///
/// Deliberately the same file the installed app uses: dev mode exists to drive
/// the real backend, and a separate profile here would hide exactly the
/// config-handling faults it is used to find. The cost is that a careless dev
/// run edits real settings, which is #68's subject rather than this module's -
/// so the path is announced on load instead.
pub fn standalone_config_path() -> PathBuf {
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

/// Load config without Tauri's AppHandle, with the durability the app has.
///
/// Recovery, quarantine and provenance all come from `load_and_report`, so a
/// config the dev server cannot read is restored from the backup rather than
/// replaced by defaults - and the session is marked as holding a fallback, so
/// its later saves cannot overwrite a config that recovers underneath it.
///
/// This writes. The loader it replaced could not touch the disk, and this one
/// refreshes the backup on a clean load and can quarantine and restore on a bad
/// one - against the installed app's profile, since they share it (#68). That is
/// the point of it rather than a side effect: a dev run against a config that
/// needs recovering should recover it, not step around it. It does mean this is
/// not a read-only way to inspect a profile, and that a router test written
/// against a developer's real `%APPDATA%` would rewrite it.
pub fn load_standalone_config(path: &Path) -> AppConfig {
    // Named on the way in, because #68 means this is very likely a real profile
    // rather than a scratch one, and nothing else on screen says so.
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
