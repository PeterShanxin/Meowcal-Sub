use crate::config::WindowPreferences;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize};

/// Restore saved geometry before first visibility to avoid a center-then-jump.
pub fn restore_and_show_main(app: &AppHandle, preferences: &WindowPreferences) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if let (Some(width), Some(height)) = (preferences.width, preferences.height) {
        let _ = window.set_size(PhysicalSize::new(width, height));
    }
    if let (Some(x), Some(y)) = (preferences.x, preferences.y) {
        let _ = window.set_position(PhysicalPosition::new(x, y));
    } else {
        let _ = window.center();
    }
    if preferences.is_maximized {
        let _ = window.maximize();
    }
    let _ = window.show();
}
