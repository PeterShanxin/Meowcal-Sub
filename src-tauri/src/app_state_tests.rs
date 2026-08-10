use super::*;

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

    state.set_capture_region(region(), 1.5);

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

    state.set_capture_region(region(), 1.0);

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
    for scale_factor in [0.0, -1.0] {
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
