//! System tray icon and menu.
//!
//! Owns the tray's menu items, their ids, and the actions they trigger. The
//! menu id → action mapping is split from the Tauri event plumbing so the
//! contract between the tray and the app can be pinned in tests.

use crate::app_profile::AppProfile;
use crate::ui_language::Locale;
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
const TRAY_ID: &str = "main";

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

fn tooltip_for(profile: AppProfile, locale: Locale) -> String {
    format!(
        "{} - {}",
        locale.brand(profile),
        locale.text("Click to show", "点击显示")
    )
}

fn menu_for(app: &AppHandle, locale: Locale) -> tauri::Result<Menu<tauri::Wry>> {
    let items = [
        (MENU_ITEM_SHOW, locale.text("Show Window", "显示窗口")),
        (
            MENU_ITEM_SELECT_AREA,
            locale.text("Select Area", "选择区域"),
        ),
        (MENU_ITEM_SETTINGS, locale.text("Settings", "设置")),
        (MENU_ITEM_QUIT, locale.text("Quit", "退出")),
    ];
    let menu = Menu::new(app)?;
    for (id, label) in items {
        menu.append(&MenuItem::with_id(app, id, label, true, None::<&str>)?)?;
    }
    Ok(menu)
}

pub fn localize(app: &AppHandle, locale: Locale) -> tauri::Result<()> {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_menu(Some(menu_for(app, locale)?))?;
        tray.set_tooltip(Some(tooltip_for(AppProfile::current(), locale)))?;
    }
    Ok(())
}

/// Build the tray icon and its menu.
///
/// The returned tray handle must stay alive for the tray to exist; binding it
/// to `_tray` in the caller is enough, matching the pre-extraction setup
/// closure. The caller decides when this runs during startup.
pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    let locale = crate::ui_language::current(app.handle());
    let menu = menu_for(app.handle(), locale)?;
    let tooltip = tooltip_for(AppProfile::current(), locale);

    // Create the tray icon
    let _tray = TrayIconBuilder::with_id(TRAY_ID)
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
    if !crate::window_startup::main_startup_allowed() {
        if let Some(window) = app.get_webview_window("foundry-wizard") {
            let _ = window.show();
            let _ = window.set_focus();
        }
        return;
    }
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
            tooltip_for(AppProfile::Production, Locale::English),
            "Meowcal Sub - Click to show"
        );
        assert_eq!(
            tooltip_for(AppProfile::Development, Locale::English),
            "Meowcal Sub - Dev - Click to show"
        );
        assert_eq!(
            tooltip_for(AppProfile::Production, Locale::SimplifiedChinese),
            "Meowcal Sub · 妙影 - 点击显示"
        );
    }
}
