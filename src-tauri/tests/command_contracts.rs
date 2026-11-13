//! Contract tests for command payloads that the frontend reads directly.
//!
//! These live outside `src/` on purpose: they pin the observable command
//! contract, so they keep passing unchanged while the implementation moves
//! between modules. A structural refactor that alters one of these shapes is a
//! visible behavior change, not an extraction.

use meowcal_sub::commands::get_system_info;
use meowcal_sub::update_diagnostics::UpdateCheckDiagnostic;

#[test]
fn system_ui_language_returns_a_supported_locale_tag() {
    let locale = meowcal_sub::ui_language::get_system_ui_language();
    assert!(matches!(locale, "en" | "zh-CN"));
    #[cfg(target_os = "windows")]
    {
        let primary_language =
            unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() } & 0x03ff;
        assert_eq!(
            locale,
            if primary_language == 0x04 {
                "zh-CN"
            } else {
                "en"
            }
        );
    }
}

#[test]
fn update_check_diagnostics_preserve_the_frontend_contract() -> Result<(), serde_json::Error> {
    let value = serde_json::json!({
        "requestId": 7,
        "source": "automatic",
        "intent": "manual",
        "phase": "finished",
        "startedAtMs": 1_700_000_000_000_u64,
        "elapsedMs": 10_000,
        "result": "timeout",
        "errorKind": "timeout"
    });
    let event: UpdateCheckDiagnostic = serde_json::from_value(value.clone())?;
    assert_eq!(serde_json::to_value(event)?, value);
    Ok(())
}

#[test]
fn display_context_uses_camel_case_and_monitor_local_geometry() {
    let value = serde_json::to_value(meowcal_sub::display_session::DisplayContext {
        multiple_displays: true,
        region: Some(meowcal_sub::config::CaptureRegion::new(100, 200, 600, 80)),
    })
    .expect("serialize display context");
    assert_eq!(
        value,
        serde_json::json!({ "multipleDisplays": true,
        "region": { "x": 100, "y": 200, "width": 600, "height": 80 } })
    );
}

/// The frontend reads `info.is_copilot_plus` and `info.windows_ocr_available`
/// directly, so this payload is snake_case even though most Tauri payloads in
/// this crate are camelCase.
#[test]
fn system_info_serializes_with_snake_case_keys() {
    let value = serde_json::to_value(get_system_info()).expect("serialize system info");
    let object = value.as_object().expect("system info is an object");

    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "arch",
            "is_copilot_plus",
            "os",
            "phi_silica_available",
            "windows_ocr_available",
        ]
    );
}

#[test]
fn system_info_reports_the_host_and_its_capabilities() {
    let info = get_system_info();

    assert_eq!(info.os, std::env::consts::OS);
    assert_eq!(info.arch, std::env::consts::ARCH);
    assert_eq!(
        info.is_copilot_plus,
        cfg!(target_arch = "aarch64") && cfg!(target_os = "windows")
    );
    assert!(!info.phi_silica_available);
    assert_eq!(info.windows_ocr_available, cfg!(target_os = "windows"));
}
