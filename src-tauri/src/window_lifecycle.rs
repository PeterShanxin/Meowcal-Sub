use crate::config::WindowPreferences;
use tauri::{
    AppHandle, CloseRequestApi, Emitter, Manager, PhysicalPosition, PhysicalSize, Runtime, Window,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseBehavior {
    HideMainToTray,
    HideWizard,
    AllowClose,
}

pub fn close_behavior(window_label: &str) -> CloseBehavior {
    match window_label {
        "main" => CloseBehavior::HideMainToTray,
        "foundry-wizard" => CloseBehavior::HideWizard,
        _ => CloseBehavior::AllowClose,
    }
}

pub fn handle_close_requested<R: Runtime>(window: &Window<R>, api: &CloseRequestApi) {
    match close_behavior(window.label()) {
        CloseBehavior::HideMainToTray => {
            api.prevent_close();
            let _ = window.hide();
        }
        CloseBehavior::HideWizard => {
            api.prevent_close();
            let _ = window.emit("wizard-window-hidden", ());
            let _ = window.app_handle().emit(
                "foundry-wizard-closed",
                serde_json::json!({
                    "modelDownloaded": false,
                    "selectedModel": null,
                    "closedViaX": true
                }),
            );
            let _ = window.hide();
        }
        CloseBehavior::AllowClose => {}
    }
}

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

#[cfg(test)]
mod tests {
    use super::{close_behavior, CloseBehavior};

    #[test]
    fn main_window_hides_to_tray_instead_of_destroying() {
        assert_eq!(close_behavior("main"), CloseBehavior::HideMainToTray);
    }

    #[test]
    fn setup_wizard_remains_reopenable() {
        assert_eq!(close_behavior("foundry-wizard"), CloseBehavior::HideWizard);
    }

    #[test]
    fn transient_windows_can_close_normally() {
        assert_eq!(close_behavior("selector"), CloseBehavior::AllowClose);
    }
}
