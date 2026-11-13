use super::*;
use crate::capture_target::CaptureMonitor;
use crate::config::CaptureRegion;

fn selected(region: CaptureRegion) -> CaptureSelection {
    CaptureSelection::new(
        region,
        CaptureMonitor {
            device_id: "internal".into(),
            bounds: CaptureRegion::new(-3120, 0, 3120, 2080),
            scale_factor: 1.5,
        },
    )
    .unwrap()
}

use crate::engine_config::ManagedLocalRuntimeConfig;

fn installed_engine() -> AppConfig {
    let mut config = AppConfig::default();
    config.translation.foundry_local.model = Some("HY-MT1.5-1.8B-Q4_K_M".to_string());
    config.translation.foundry_local.endpoint_url = Some("http://127.0.0.1:11436".to_string());
    config.translation.foundry_local.managed_runtime = Some(ManagedLocalRuntimeConfig {
        kind: "hy-mt".to_string(),
        executable_path: r"D:\engine\llama-server.exe".to_string(),
        model_path: r"D:\engine\model.gguf".to_string(),
        port: 11436,
    });
    config
}

fn preferences(width: u32) -> WindowPreferences {
    WindowPreferences {
        width: Some(width),
        height: Some(600),
        x: Some(10),
        y: Some(20),
        scale_factor: Some(1.0),
        is_maximized: false,
    }
}

/// The settings form has no region picker; the region comes from the selector
/// and has to survive an unrelated settings save.
#[test]
fn the_live_region_monitor_and_scale_replace_whatever_was_submitted() {
    let submitted = AppConfig {
        last_capture_region: Some(CaptureRegion::new(0, 0, 1, 1)),
        last_capture_scale_factor: Some(9.0),
        last_capture_monitor: Some(CaptureMonitor {
            device_id: "external".into(),
            bounds: CaptureRegion::new(0, 0, 2560, 1440),
            scale_factor: 1.25,
        }),
        ..AppConfig::default()
    };
    let region = CaptureRegion::new(10, 20, 640, 100);

    let merged = merge_app_owned_state(
        submitted,
        Some(&selected(region)),
        None,
        &AppConfig::default(),
    );

    assert_eq!(merged.last_capture_region, Some(region));
    assert_eq!(merged.last_capture_scale_factor, Some(1.5));
    assert_eq!(merged.last_capture_monitor, Some(selected(region).monitor));
}

#[test]
fn no_selected_region_clears_the_stored_one() {
    let submitted = AppConfig {
        last_capture_region: Some(CaptureRegion::new(0, 0, 1, 1)),
        last_capture_monitor: Some(selected(CaptureRegion::new(0, 0, 1, 1)).monitor),
        ..AppConfig::default()
    };

    let merged = merge_app_owned_state(submitted, None, None, &AppConfig::default());

    assert_eq!(merged.last_capture_region, None);
    assert_eq!(merged.last_capture_monitor, None);
    assert_eq!(merged.last_capture_scale_factor, None);
}

#[test]
fn measured_window_geometry_replaces_the_submitted_geometry() {
    let submitted = AppConfig {
        window_preferences: preferences(100),
        ..AppConfig::default()
    };

    let merged = merge_app_owned_state(
        submitted,
        None,
        Some(preferences(1280)),
        &AppConfig::default(),
    );

    assert_eq!(merged.window_preferences.width, Some(1280));
}

/// Without a main window there is nothing to measure, so the submitted
/// geometry is left alone rather than being blanked.
#[test]
fn geometry_is_left_alone_when_there_is_no_window_to_measure() {
    let submitted = AppConfig {
        window_preferences: preferences(100),
        ..AppConfig::default()
    };

    let merged = merge_app_owned_state(submitted, None, None, &AppConfig::default());

    assert_eq!(merged.window_preferences.width, Some(100));
}

/// The engine registration is app-owned. A settings form that does not know
/// about it must not be able to erase a working install (#69).
#[test]
fn an_installed_engine_survives_settings_that_do_not_mention_it() {
    let live = installed_engine();

    let merged = merge_app_owned_state(AppConfig::default(), None, None, &live);

    assert_eq!(
        merged.translation.foundry_local.model,
        live.translation.foundry_local.model
    );
    assert_eq!(
        merged.translation.foundry_local.endpoint_url,
        live.translation.foundry_local.endpoint_url
    );
    assert!(merged.translation.foundry_local.managed_runtime.is_some());
}

#[test]
fn user_editable_settings_still_come_from_the_submitted_config() {
    let submitted = AppConfig {
        target_language: "ja-JP".to_string(),
        capture_interval_ms: 1234,
        ..AppConfig::default()
    };

    let merged = merge_app_owned_state(submitted, None, None, &installed_engine());

    assert_eq!(merged.target_language, "ja-JP");
    assert_eq!(merged.capture_interval_ms, 1234);
}

#[test]
fn stale_window_settings_cannot_replace_the_live_interface_language() {
    use crate::ui_language::UiLanguage;
    let live = AppConfig {
        ui_language: UiLanguage::SimplifiedChinese,
        ..AppConfig::default()
    };
    let submitted = AppConfig {
        ui_language: UiLanguage::English,
        target_language: "ja-JP".into(),
        ..AppConfig::default()
    };
    let merged = merge_app_owned_state(submitted, None, None, &live);
    assert_eq!(merged.ui_language, UiLanguage::SimplifiedChinese);
    assert_eq!(merged.target_language, "ja-JP");
}
