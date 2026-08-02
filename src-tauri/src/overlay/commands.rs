// =============================================================================
// OVERLAY WINDOW COMMANDS - shape and input behaviour of the floating overlay
// =============================================================================
// These live beside the window code they drive rather than in the general
// command surface: both of them exist only because the overlay is a chromeless
// window whose translucency and hit-testing are managed by hand on Windows.
// =============================================================================

use tauri::{AppHandle, Manager};
use tracing::warn;

use crate::config::CaptureRegion;

/// Set whether the overlay window ignores cursor events (click-through)
///
/// When `ignore` is true, the overlay is click-through (default during translation).
/// When `ignore` is false, the overlay can receive mouse events (for settings interaction).
///
/// Called from JavaScript: `await invoke('set_overlay_click_through', { ignore: false });`
#[tauri::command]
pub fn set_overlay_click_through(app: AppHandle, ignore: bool) -> Result<(), String> {
    let window = app
        .get_webview_window("overlay")
        .ok_or("Overlay window not found")?;

    window
        .set_ignore_cursor_events(ignore)
        .map_err(|e| format!("Failed to set cursor events: {}", e))?;

    // set_ignore_cursor_events owns WS_EX_LAYERED as well, so turning
    // click-through off strips the overlay's translucency with it.
    if let Err(e) = super::window_alpha::apply(&window) {
        warn!("Failed to restore overlay translucency: {}", e);
    }

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
/// `invoke('set_overlay_window_clip', { frameRegion, subtitleBounds, handleBounds, controlRadii, scaleFactor })`
#[tauri::command]
pub fn set_overlay_window_clip(
    app: AppHandle,
    frame_region: Option<CaptureRegion>,
    subtitle_bounds: Option<CaptureRegion>,
    handle_bounds: Option<Vec<CaptureRegion>>,
    control_radii: Option<Vec<f64>>,
    scale_factor: f64,
) -> Result<(), String> {
    let window = app
        .get_webview_window("overlay")
        .ok_or("Overlay window not found")?;

    super::window_clip::apply_overlay_window_clip(
        &window,
        frame_region,
        subtitle_bounds,
        handle_bounds,
        control_radii,
        scale_factor,
    )?;

    Ok(())
}
