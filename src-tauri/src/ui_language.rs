//! Interface language is independent of the languages used for subtitles.

use crate::{app_profile::AppProfile, app_state::AppState, sync_utils::lock_or_recover};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiLanguage {
    #[default]
    #[serde(rename = "system")]
    System,
    #[serde(rename = "en")]
    English,
    #[serde(rename = "zh-CN")]
    SimplifiedChinese,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    English,
    SimplifiedChinese,
}

impl UiLanguage {
    pub fn resolve(self) -> Locale {
        self.resolve_with(system_locale())
    }

    fn resolve_with(self, system: Locale) -> Locale {
        match self {
            Self::System => system,
            Self::English => Locale::English,
            Self::SimplifiedChinese => Locale::SimplifiedChinese,
        }
    }
}

impl Locale {
    pub fn text(self, english: &'static str, chinese: &'static str) -> &'static str {
        match self {
            Self::English => english,
            Self::SimplifiedChinese => chinese,
        }
    }

    pub fn brand(self, profile: AppProfile) -> String {
        let brand = self.text("Meowcal Sub", "Meowcal Sub · 妙影");
        match profile {
            AppProfile::Development | AppProfile::StoreDevelopment => format!("{brand} - Dev"),
            _ => brand.to_string(),
        }
    }

    pub fn window_title(self, label: &str) -> String {
        let brand = self.brand(AppProfile::current());
        let surface = match label {
            "foundry-wizard" => self.text("Setup", "初始设置"),
            "selector" => self.text("Select subtitle area", "选择字幕区域"),
            "overlay" => self.text("Subtitles", "字幕"),
            _ => return brand,
        };
        format!("{brand} — {surface}")
    }
}

#[cfg(any(target_os = "windows", test))]
fn locale_from_language_id(language: u16) -> Locale {
    // LANGID's low ten bits identify the primary language, including all
    // Chinese regions and scripts. Only Simplified Chinese is supported.
    if language & 0x03ff == 0x04 {
        Locale::SimplifiedChinese
    } else {
        Locale::English
    }
}

fn system_locale() -> Locale {
    #[cfg(target_os = "windows")]
    {
        // No pointers or buffers: this reads the current user's Windows UI language.
        locale_from_language_id(unsafe {
            windows::Win32::Globalization::GetUserDefaultUILanguage()
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        Locale::English
    }
}

/// Use the same Windows display language for webviews and native surfaces.
#[tauri::command]
pub fn get_system_ui_language() -> &'static str {
    system_locale().text("en", "zh-CN")
}

#[tauri::command]
pub async fn set_ui_language(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    language: UiLanguage,
) -> Result<(), String> {
    crate::settings_service::save_language(app, &state, language).await
}

pub fn current(app: &AppHandle) -> Locale {
    let state = app.state::<AppState>();
    let preference = lock_or_recover(&state.config).ui_language;
    preference.resolve()
}

/// Apply only after a successful config write, so other windows cannot adopt
/// a preference that failed to save. The payload is the stored preference.
pub fn apply(app: &AppHandle, language: UiLanguage) {
    let locale = language.resolve();
    for (label, window) in app.webview_windows() {
        if let Err(error) = window.set_title(&locale.window_title(&label)) {
            tracing::warn!("Could not localize {label} window title: {error}");
        }
    }
    if let Err(error) = crate::tray::localize(app, locale) {
        tracing::warn!("Could not localize tray: {error}");
    }
    if let Err(error) = app.emit("ui-language-changed", language) {
        tracing::warn!("Could not publish interface language: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_preferences_override_system_language() {
        assert_eq!(
            UiLanguage::English.resolve_with(Locale::SimplifiedChinese),
            Locale::English
        );
        assert_eq!(
            UiLanguage::SimplifiedChinese.resolve_with(Locale::English),
            Locale::SimplifiedChinese
        );
        assert_eq!(
            UiLanguage::System.resolve_with(Locale::SimplifiedChinese),
            Locale::SimplifiedChinese
        );
        assert_eq!(
            UiLanguage::System.resolve_with(Locale::English),
            Locale::English
        );
    }

    #[test]
    fn chinese_regions_resolve_to_the_supported_chinese_locale() {
        for language in [0x0804, 0x0404, 0x0c04, 0x1004, 0x1404] {
            assert_eq!(locale_from_language_id(language), Locale::SimplifiedChinese);
        }
        for language in [0x0409, 0x0809, 0x0411, 0x040c, 0] {
            assert_eq!(locale_from_language_id(language), Locale::English);
        }
    }

    #[test]
    fn brands_keep_the_development_suffix_and_localized_name() {
        assert_eq!(Locale::English.brand(AppProfile::Production), "Meowcal Sub");
        assert_eq!(
            Locale::SimplifiedChinese.brand(AppProfile::Store),
            "Meowcal Sub · 妙影"
        );
        assert_eq!(
            Locale::SimplifiedChinese.brand(AppProfile::Development),
            "Meowcal Sub · 妙影 - Dev"
        );
    }
}
