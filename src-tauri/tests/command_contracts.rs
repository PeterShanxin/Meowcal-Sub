//! Contract tests for command payloads that the frontend reads directly.
//!
//! These live outside `src/` on purpose: they pin the observable command
//! contract, so they keep passing unchanged while the implementation moves
//! between modules. A structural refactor that alters one of these shapes is a
//! visible behavior change, not an extraction.

use meowcal_sub::commands::get_system_info;

/// `main.js` reads `info.is_copilot_plus` and `info.windows_ocr_available`
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
