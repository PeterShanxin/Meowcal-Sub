use super::*;

fn selection(scale: f64) -> crate::capture_target::CaptureSelection {
    crate::capture_target::CaptureSelection::new(
        region(),
        crate::capture_target::CaptureMonitor {
            device_id: "screen".into(),
            bounds: CaptureRegion::new(0, 0, 1920, 1080),
            scale_factor: scale,
        },
    )
    .unwrap()
}

fn region() -> CaptureRegion {
    CaptureRegion::new(10, 20, 640, 100)
}

#[test]
fn a_fresh_state_has_no_region_and_no_scaling() {
    let state = AppState::default();

    assert_eq!(state.current_capture_region(), None);
    assert_eq!(state.capture_scale_factor(), 1.0);
    assert!(!state.is_translation_running());
}

#[test]
fn setting_a_region_records_it_with_its_scale_factor() {
    let state = AppState::default();

    state.set_capture_selection(selection(1.5));

    assert_eq!(state.current_capture_region(), Some(region()));
    assert_eq!(state.capture_scale_factor(), 1.5);
}

/// A frame captured from the previous region has already been framed, and may
/// already be at the translator. Letting it land would paint the old area's
/// subtitle over the new one.
#[test]
fn setting_a_region_invalidates_work_already_in_flight() {
    let state = AppState::default();
    let session = state.pipeline_clock.begin_session();
    let in_flight = state.pipeline_clock.next_capture(session);
    assert!(state.pipeline_clock.is_current(in_flight));

    state.set_capture_selection(selection(1.0));

    assert!(!state.pipeline_clock.is_current(in_flight));
    assert!(
        state.pipeline_clock.is_session_current(session),
        "the session survives a region change; only the frame is stale"
    );
}

#[test]
fn a_region_with_no_area_is_refused() {
    for (width, height) in [(0, 100), (100, 0), (-1, 100), (100, -1)] {
        assert_eq!(
            validate_capture_region(width, height, 1.0),
            Err("Width and height must be positive".to_string()),
            "expected {width}x{height} to be refused"
        );
    }
}

#[test]
fn a_non_positive_scale_factor_is_refused() {
    for scale_factor in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            validate_capture_region(100, 100, scale_factor),
            Err("Scale factor must be positive".to_string()),
            "expected scale {scale_factor} to be refused"
        );
    }
}

#[test]
fn a_capturable_region_is_accepted() {
    assert_eq!(validate_capture_region(1, 1, 0.5), Ok(()));
    assert_eq!(validate_capture_region(1920, 1080, 2.0), Ok(()));
}

#[test]
fn display_loss_invalidates_pending_work_and_signals_stop() {
    let state = AppState::default();
    state.set_capture_selection(selection(1.5));
    let session = state.pipeline_clock.begin_session();
    let capture = state.pipeline_clock.next_capture(session);
    let translation = state.pipeline_clock.begin_translation();
    let (stop, receiver) = watch::channel(false);
    *lock_or_recover(&state.stop_signal) = Some(stop);
    assert!(state.lose_capture_display(session));
    assert!(state.current_capture_selection().is_none());
    assert!(!state.pipeline_clock.is_current(capture));
    assert!(!state.pipeline_clock.is_translation_current(translation));
    assert!(*receiver.borrow());
}

#[test]
fn a_delayed_display_probe_cannot_stop_a_new_session() {
    let state = AppState::default();
    let old = state.pipeline_clock.begin_session();
    let new = state.pipeline_clock.begin_session();
    state.set_capture_selection(selection(2.0));
    assert!(!state.lose_capture_display(old));
    assert_eq!(state.current_capture_selection(), Some(selection(2.0)));
    assert!(state.pipeline_clock.is_session_current(new));
}

// Commands can arrive before Tauri setup has read the settings file. They
// must see the loaded settings, never the in-memory defaults (an installed
// engine read from defaults looks uninstalled).
#[tokio::test]
async fn loaded_config_waits_for_startup_and_returns_the_loaded_settings() {
    let state = std::sync::Arc::new(AppState::default());
    let reader = std::sync::Arc::clone(&state);
    let early = tokio::spawn(async move { reader.loaded_config().await });

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    assert!(!early.is_finished());

    lock_or_recover(&state.config).capture_interval_ms = 1234;
    state.startup_gate.mark_ready();

    assert_eq!(early.await.unwrap().unwrap().capture_interval_ms, 1234);
}
