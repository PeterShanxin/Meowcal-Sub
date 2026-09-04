//! System tray icon and menu.
//!
//! Owns the tray's menu items, their ids, and the actions they trigger. The
//! menu id → action mapping is split from the Tauri event plumbing so the
//! contract between the tray and the app can be pinned in tests.

use crate::app_profile::AppProfile;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};
use tracing::info;

const MENU_ITEM_SHOW: &str = "show";
const MENU_ITEM_SELECT_AREA: &str = "select_area";
const MENU_ITEM_SETTINGS: &str = "settings";
const MENU_ITEM_QUIT: &str = "quit";

/// The actions the tray menu can trigger, keyed by menu item id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayAction {
    Show,
    SelectArea,
    Settings,
    Quit,
}

/// Map a menu item id to the action it stands for.
fn action_for_menu_id(id: &str) -> Option<TrayAction> {
    match id {
        MENU_ITEM_SHOW => Some(TrayAction::Show),
        MENU_ITEM_SELECT_AREA => Some(TrayAction::SelectArea),
        MENU_ITEM_SETTINGS => Some(TrayAction::Settings),
        MENU_ITEM_QUIT => Some(TrayAction::Quit),
        _ => None,
    }
}

fn tooltip_for(profile: AppProfile) -> String {
    format!("{} - Click to show", profile.display_name())
}

/// Build the tray icon and its menu.
///
/// The returned tray handle must stay alive for the tray to exist; binding it
/// to `_tray` in the caller is enough, matching the pre-extraction setup
/// closure. The caller decides when this runs during startup.
pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, MENU_ITEM_SHOW, "Show Window", true, None::<&str>)?;
    let select_area_item = MenuItem::with_id(
        app,
        MENU_ITEM_SELECT_AREA,
        "Select Area",
        true,
        None::<&str>,
    )?;
    let settings_item = MenuItem::with_id(app, MENU_ITEM_SETTINGS, "Settings", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, MENU_ITEM_QUIT, "Quit", true, None::<&str>)?;

    // Build the tray menu
    let menu = Menu::with_items(
        app,
        &[&show_item, &select_area_item, &settings_item, &quit_item],
    )?;

    let tooltip = tooltip_for(AppProfile::current());

    // Create the tray icon
    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .tooltip(&tooltip)
        .on_menu_event(|app, event| {
            if let Some(action) = action_for_menu_id(event.id.as_ref()) {
                handle_action(app, action);
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button,
                button_state,
                ..
            } = event
            {
                if button == MouseButton::Left && button_state == MouseButtonState::Up {
                    show_main_window(tray.app_handle());
                }
            }
        })
        .build(app)?;

    info!("✅ System tray set up successfully!");
    Ok(())
}

/// Run the action a menu item stands for.
fn handle_action(app: &AppHandle, action: TrayAction) {
    match action {
        TrayAction::Show => {
            info!("Tray: Show window clicked");
            show_main_window(app);
        }
        TrayAction::SelectArea => {
            info!("Tray: Select area clicked");
            // TODO: Implement area selection
        }
        TrayAction::Settings => {
            info!("Tray: Settings clicked");
            show_main_window(app);
        }
        TrayAction::Quit => {
            info!("Tray: Quit clicked");
            app.exit(0);
        }
    }
}

/// Bring the main window to the front, as the tray Show/Settings actions do.
fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_menu_item_maps_to_an_action() {
        assert_eq!(action_for_menu_id(MENU_ITEM_SHOW), Some(TrayAction::Show));
        assert_eq!(
            action_for_menu_id(MENU_ITEM_SELECT_AREA),
            Some(TrayAction::SelectArea)
        );
        assert_eq!(
            action_for_menu_id(MENU_ITEM_SETTINGS),
            Some(TrayAction::Settings)
        );
        assert_eq!(action_for_menu_id(MENU_ITEM_QUIT), Some(TrayAction::Quit));
    }

    #[test]
    fn unknown_menu_ids_are_ignored() {
        assert_eq!(action_for_menu_id("bogus"), None);
        assert_eq!(action_for_menu_id(""), None);
    }

    #[test]
    fn tray_tooltip_identifies_only_development_builds() {
        assert_eq!(
            tooltip_for(AppProfile::Production),
            "Meowcal Sub - Click to show"
        );
        assert_eq!(
            tooltip_for(AppProfile::Development),
            "Meowcal Sub - Dev - Click to show"
        );
    }
}
