//! Reading and writing the settings the UI edits.
//!
//! The submitted settings are not the whole config. Capture region, DPI scale,
//! window geometry, and the installed engine's registration are owned by the
//! app, not by the settings form, so they are folded back in before anything
//! is written. `config_save` owns durability from there.

use tauri::{async_runtime, AppHandle, Manager};
use tracing::{info, warn};

use crate::app_state::AppState;
use crate::capture_target::CaptureSelection;
use crate::config::{save_config, AppConfig, WindowPreferences};
use crate::sync_utils::lock_or_recover;

/// The settings currently in effect, once startup has finished loading them.
pub async fn current(state: &AppState) -> Result<AppConfig, String> {
    state.startup_gate.wait_until_ready().await?;
    info!("Getting settings...");
    let config = lock_or_recover(&state.config);
    Ok(config.clone())
}

/// Apply submitted settings to the running app and persist them.
pub async fn save(app: AppHandle, state: &AppState, settings: AppConfig) -> Result<(), String> {
    state.startup_gate.wait_until_ready().await?;
    info!("Saving settings...");

    // Through the same path the window events use, rather than reading the
    // window here: that copy lacked the minimised and offscreen guards, so
    // saving any setting while minimised stored the minimised geometry.
    let main_window_present = match app.get_webview_window("main") {
        Some(window) => {
            crate::window_lifecycle::remember_main_geometry(&window.as_ref().window());
            true
        }
        None => false,
    };

    let app_handle = app.clone();
    let updated = async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        crate::config_save::update_live(
            &state.config,
            |config| {
                let selection = state.current_capture_selection();
                let preferences = main_window_present.then(|| config.window_preferences.clone());
                *config = merge_app_owned_state(settings, selection.as_ref(), preferences, config);
            },
            |config| save_config(&app_handle, config),
        )
    })
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

    crate::core_client::select_cpu_only(updated.translation.foundry_local.cpu_only)
}

/// Fold app-owned state back into the settings the UI submitted.
///
/// `window_preferences` is `None` when there is no main window to measure, in
/// which case whatever the form submitted stands.
fn merge_app_owned_state(
    submitted: AppConfig,
    selection: Option<&CaptureSelection>,
    window_preferences: Option<WindowPreferences>,
    live: &AppConfig,
) -> AppConfig {
    let mut updated = submitted;

    updated.last_capture_region = selection.map(|s| s.region);
    updated.last_capture_scale_factor = selection.map(|s| s.monitor.scale_factor);
    updated.last_capture_monitor = selection.map(|s| s.monitor.clone());

    if let Some(preferences) = window_preferences {
        updated.window_preferences = preferences;
    }

    updated
        .translation
        .foundry_local
        .preserve_managed_runtime_from(&live.translation.foundry_local);

    updated
}

#[cfg(test)]
#[path = "settings_service_tests.rs"]
mod tests;
