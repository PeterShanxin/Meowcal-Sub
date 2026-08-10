//! Reading and writing the settings the UI edits.
//!
//! The submitted settings are not the whole config. Capture region, DPI scale,
//! window geometry, and the installed engine's registration are owned by the
//! app, not by the settings form, so they are folded back in before anything
//! is written. `config_save` owns durability from there.

use tauri::{async_runtime, AppHandle, Manager};
use tracing::{info, warn};

use crate::app_state::AppState;
use crate::config::{save_config, AppConfig, CaptureRegion, WindowPreferences};
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
    info!("Saving settings...");

    let last_region = state.current_capture_region();
    let last_scale_factor = state.capture_scale_factor();

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

    // Update the in-memory config
    let updated = {
        let mut config = lock_or_recover(&state.config);
        let window_preferences = main_window_present.then(|| config.window_preferences.clone());
        let merged = merge_app_owned_state(
            settings,
            last_region,
            last_scale_factor,
            window_preferences,
            &config,
        );
        *config = merged.clone();
        merged
    };

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

/// Fold app-owned state back into the settings the UI submitted.
///
/// `window_preferences` is `None` when there is no main window to measure, in
/// which case whatever the form submitted stands.
fn merge_app_owned_state(
    submitted: AppConfig,
    last_capture_region: Option<CaptureRegion>,
    last_capture_scale_factor: f64,
    window_preferences: Option<WindowPreferences>,
    live: &AppConfig,
) -> AppConfig {
    let mut updated = submitted;

    // Persist the last capture region into the config
    updated.last_capture_region = last_capture_region;

    // Persist the DPI scale factor so restored regions capture the correct physical pixels.
    updated.last_capture_scale_factor = Some(last_capture_scale_factor);

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
