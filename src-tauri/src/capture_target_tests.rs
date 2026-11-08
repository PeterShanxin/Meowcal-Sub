use super::*;

fn monitor(id: &str, x: i32, y: i32, width: i32, height: i32, scale: f64) -> CaptureMonitor {
    CaptureMonitor {
        device_id: id.into(),
        bounds: CaptureRegion::new(x, y, width, height),
        scale_factor: scale,
    }
}

fn screens() -> Vec<CaptureMonitor> {
    vec![
        monitor("external", 0, 0, 2560, 1440, 1.25),
        monitor("internal", -3120, -520, 3120, 2080, 2.0),
    ]
}

#[test]
fn negative_desktop_origins_are_not_scaled_with_local_coordinates() {
    let selection =
        CaptureSelection::new(CaptureRegion::new(100, 120, 600, 80), screens()[1].clone()).unwrap();
    assert_eq!(
        selection.physical_local().unwrap(),
        CaptureRegion::new(200, 240, 1200, 160)
    );
    let local = selection.physical_local().unwrap();
    assert_eq!(
        (
            selection.monitor.bounds.x + local.x,
            selection.monitor.bounds.y + local.y
        ),
        (-2920, -280)
    );
}

#[test]
fn dpi_follows_the_selected_monitor() {
    let area = CaptureRegion::new(100, 120, 600, 80);
    assert_eq!(
        CaptureSelection::new(area, screens()[0].clone())
            .unwrap()
            .physical_local()
            .unwrap(),
        CaptureRegion::new(125, 150, 750, 100)
    );
    assert_eq!(
        CaptureSelection::new(area, screens()[1].clone())
            .unwrap()
            .physical_local()
            .unwrap()
            .width,
        1200
    );
}

#[test]
fn fractional_viewport_edges_are_trimmed_without_moving_the_region() {
    let target = monitor("fractional", 2500, 300, 1920, 1080, 1.75);
    let selection = CaptureSelection::new(CaptureRegion::new(1000, 500, 98, 118), target).unwrap();
    assert_eq!(
        selection.physical_local().unwrap(),
        CaptureRegion::new(1750, 875, 170, 205)
    );
}

#[test]
fn invalid_areas_do_not_slide_onto_another_screen() {
    for region in [
        CaptureRegion::new(-1, 0, 40, 40),
        CaptureRegion::new(1500, 900, 600, 90),
        CaptureRegion::new(0, 0, 0, 30),
        CaptureRegion::new(i32::MAX, 0, i32::MAX, 30),
    ] {
        assert_eq!(
            CaptureSelection::new(region, screens()[1].clone()),
            Err(TargetError::OutsideMonitor)
        );
    }
    for scale in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(CaptureSelection::new(
            CaptureRegion::new(0, 0, 10, 10),
            monitor("invalid", 0, 0, 100, 100, scale)
        )
        .is_err());
    }
}

#[test]
fn reconnect_primary_change_resolution_and_dpi_require_surface_revalidation() {
    let original = screens()[1].clone();
    assert!(!original.is_present(&screens()[..1]));
    for changed in [
        monitor("replacement", -3120, -520, 3120, 2080, 2.0),
        monitor("internal", 0, 0, 3120, 2080, 2.0),
        monitor("internal", -3120, -520, 2560, 1440, 2.0),
        monitor("internal", -3120, -520, 3120, 2080, 1.5),
    ] {
        assert!(!original.is_present(&[changed]));
    }
    assert!(original.is_present(&screens()));
}

#[test]
fn cancelled_or_reopened_selectors_cannot_commit_late_results() {
    let mut state = CaptureState::default();
    let old = state.begin_selection(screens()[0].clone());
    let current = state.begin_selection(screens()[1].clone());
    let area = CaptureRegion::new(20, 40, 600, 100);
    assert_eq!(
        state.confirm(old, area, &screens()),
        Err(TargetError::ExpiredSelection)
    );
    state.cancel_selection();
    assert_eq!(
        state.confirm(current, area, &screens()),
        Err(TargetError::ExpiredSelection)
    );
    assert!(state.selected.is_none());
}

#[test]
fn hotplug_during_selection_leaves_previous_selection_unchanged() {
    let mut state = CaptureState::default();
    let area = CaptureRegion::new(20, 40, 600, 100);
    let id = state.begin_selection(screens()[0].clone());
    let original = state.confirm(id, area, &screens()).unwrap();
    let id = state.begin_selection(screens()[1].clone());
    assert_eq!(
        state.confirm(id, area, &screens()[..1]),
        Err(TargetError::DisplayChanged)
    );
    assert_eq!(state.selected, Some(original));
}

#[test]
fn idle_main_window_moves_do_not_rebase_saved_content_to_a_new_screen() {
    let mut state = CaptureState::default();
    let area = CaptureRegion::new(20, 40, 600, 100);
    let id = state.begin_selection(screens()[1].clone());
    state.confirm(id, area, &screens()).unwrap();
    assert_eq!(
        state.visible_region(&screens()[1], false, &screens()),
        Some(area)
    );
    assert_eq!(state.visible_region(&screens()[0], false, &screens()), None);
    assert_eq!(state.selected.as_ref().unwrap().monitor, screens()[1]);
}

#[test]
fn restore_requires_identity_bounds_dpi_and_valid_local_geometry() {
    let mut state = CaptureState::default();
    let area = Some(CaptureRegion::new(20, 40, 600, 100));
    state.restore(area, Some(screens()[1].clone()), Some(2.0), &screens());
    assert_eq!(state.visible_region(&screens()[1], false, &screens()), area);
    state.restore(area, Some(screens()[1].clone()), Some(2.0), &screens()[..1]);
    assert!(state.selected.is_none());
    state.restore(
        Some(CaptureRegion::new(-10, 40, 600, 100)),
        Some(screens()[1].clone()),
        Some(2.0),
        &screens(),
    );
    assert!(state.selected.is_none());
}

#[test]
fn legacy_single_display_configs_restore_but_ambiguous_multi_display_configs_do_not() {
    let mut state = CaptureState::default();
    let area = Some(CaptureRegion::new(20, 40, 600, 100));
    state.restore(area, None, Some(1.25), &screens()[..1]);
    assert_eq!(state.visible_region(&screens()[0], false, &screens()), area);
    state.restore(area, None, None, &screens()[..1]);
    assert_eq!(state.visible_region(&screens()[0], false, &screens()), area);
    state.restore(area, None, Some(2.0), &screens()[..1]);
    assert!(state.selected.is_none());
    state.restore(area, None, Some(1.25), &screens());
    assert!(state.selected.is_none());
}

#[test]
fn persisted_monitor_identity_round_trips_with_the_selection() {
    let selection =
        CaptureSelection::new(CaptureRegion::new(20, 40, 600, 100), screens()[1].clone()).unwrap();
    let json = serde_json::to_string(&selection).unwrap();
    let restored: CaptureSelection = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, selection);
}

#[test]
fn running_session_stays_on_its_monitor_after_home_moves() {
    let mut state = CaptureState::default();
    let area = CaptureRegion::new(20, 40, 600, 100);
    let id = state.begin_selection(screens()[1].clone());
    let selected = state.confirm(id, area, &screens()).unwrap();
    assert_eq!(
        state.visible_region(&screens()[0], true, &screens()),
        Some(area)
    );
    assert_eq!(state.visible_region(&screens()[0], false, &screens()), None);
    assert_eq!(
        selected.validate_start(&screens()[0], &screens()),
        Err(TargetError::SelectOnCurrentDisplay)
    );
    assert_eq!(selected.validate_start(&screens()[1], &screens()), Ok(()));
    assert_eq!(
        state.visible_region(&screens()[0], true, &screens()[..1]),
        None
    );
    assert_eq!(
        selected.validate_start(&screens()[0], &screens()[..1]),
        Err(TargetError::DisplayChanged)
    );
}

#[test]
fn gdi_desktop_coordinates_preserve_negative_origins_and_refuse_cross_screen_capture() {
    let target = screens()[1].clone();
    assert_eq!(
        target
            .desktop_region(CaptureRegion::new(200, 240, 1200, 160))
            .unwrap(),
        CaptureRegion::new(-2920, -280, 1200, 160)
    );
    assert_eq!(
        target.desktop_region(CaptureRegion::new(3000, 0, 200, 100)),
        Err(TargetError::OutsideMonitor)
    );
}

#[test]
fn preparing_a_selection_does_not_commit_it_before_persistence_succeeds() {
    let mut state = CaptureState::default();
    let id = state.begin_selection(screens()[1].clone());
    let area = CaptureRegion::new(20, 40, 600, 100);
    let prepared = state.selection_to_confirm(id, area, &screens()).unwrap();
    assert_eq!(prepared.monitor, screens()[1]);
    assert_eq!(prepared.region, area);
    assert!(state.selected.is_none());
    state.cancel_selection();
    assert_eq!(
        state.selection_to_confirm(id, area, &screens()),
        Err(TargetError::ExpiredSelection)
    );
}
