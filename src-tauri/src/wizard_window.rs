//! Showing and hiding the setup wizard window.
//!
//! Closing it is not destroying it: `window_lifecycle::close_behavior` hides
//! the wizard so it can be reopened, and both paths tell the main window the
//! wizard is gone so it can refresh engine status.

use tauri::{AppHandle, Emitter, Manager};
use tracing::info;

/// Show the foundry-wizard window, resetting state for a fresh run
pub fn open(app: &AppHandle) -> Result<(), String> {
    info!("Opening Foundry setup wizard");
    let Some(window) = app.get_webview_window("foundry-wizard") else {
        return Err("Wizard window not found".to_string());
    };

    // Emit reset event so the wizard JS resets to step 1 and clears timers
    let _ = window.emit("wizard-reset", ());
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    window.center().map_err(|e| e.to_string())?;
    Ok(())
}

/// Hide the foundry-wizard window and notify the main window
pub fn close(
    app: &AppHandle,
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
