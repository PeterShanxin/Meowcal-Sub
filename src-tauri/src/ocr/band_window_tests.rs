use super::*;

const INTERVAL: u64 = 250;

fn record(band: &mut TrackedBand, text: &str, at: u64) {
    band.record(1000.0, 600.0, 900.0, text, at);
}

#[test]
fn observations_older_than_the_window_are_forgotten() {
    let mut band = TrackedBand::new(1000.0, INTERVAL);
    record(&mut band, "A quiet morning", 0);
    record(&mut band, "A quiet morning", WINDOW_MS / 2);
    assert_eq!(band.stats(INTERVAL).observations, 2);
    record(&mut band, "A quiet morning", WINDOW_MS + 1);
    assert_eq!(band.stats(INTERVAL).observations, 2);
}

#[test]
fn a_band_drifts_toward_where_it_is_actually_seen() {
    let mut band = TrackedBand::new(1000.0, INTERVAL);
    for at in 0..40 {
        band.record(1010.0, 600.0, 900.0, "A quiet morning", at * INTERVAL);
    }
    assert!(band.centre_y() > 1008.0);
    assert!(band.centre_y() <= 1010.0);
}

#[test]
fn a_left_aligned_band_is_judged_on_its_left_edge() {
    let mut band = TrackedBand::new(1000.0, INTERVAL);
    for (index, right) in [700.0, 1100.0, 850.0, 1300.0, 900.0].iter().enumerate() {
        band.record(
            1000.0,
            300.0,
            *right,
            "A quiet morning",
            index as u64 * INTERVAL,
        );
    }
    assert_eq!(band.stats(INTERVAL).centre_scatter, 0.0);
}

#[test]
fn a_band_seen_once_has_no_scatter_or_confirmed_cue() {
    let mut band = TrackedBand::new(1000.0, INTERVAL);
    record(&mut band, "A quiet morning", 0);
    assert_eq!(band.stats(INTERVAL).centre_scatter, 0.0);
    assert_eq!(band.stats(INTERVAL).cues, 0);
}

#[test]
fn holding_the_same_reading_counts_as_one_cue() {
    let mut band = TrackedBand::new(1000.0, INTERVAL);
    for frame in 0..20 {
        record(&mut band, "A quiet morning", frame * INTERVAL);
    }
    assert_eq!(band.stats(INTERVAL).cues, 1);
}

#[test]
fn confirmed_changes_count_without_geometry_changes() {
    let mut band = TrackedBand::new(1000.0, INTERVAL);
    for (cue, text) in ["The morning train", "A distant memory", "Please wait here"]
        .iter()
        .enumerate()
    {
        for frame in 0..8 {
            record(&mut band, text, (cue as u64 * 8 + frame) * INTERVAL);
        }
    }
    assert_eq!(band.stats(INTERVAL).cues, 3);
}

#[test]
fn scatter_demotion_uses_elapsed_time() {
    let mut band = TrackedBand::new(1000.0, INTERVAL);
    for at in [0, 250, 500] {
        record(&mut band, "Please wait here", at);
        band.settle(Verdict::Subtitle, at);
    }
    assert_eq!(band.settle(Verdict::Scattered, 600), Verdict::Subtitle);
    assert_eq!(band.settle(Verdict::Scattered, 700), Verdict::Subtitle);
    assert_eq!(band.settle(Verdict::Scattered, 2_600), Verdict::Scattered);
    assert_eq!(band.settle(Verdict::Subtitle, 2_700), Verdict::Subtitle);
}

#[test]
fn timestamps_measure_screen_time_without_counting_blank_gaps() {
    let mut band = TrackedBand::new(1000.0, INTERVAL);
    for at in [0, 800, 1_900] {
        record(&mut band, "Please wait here", at);
    }
    assert_eq!(band.stats(INTERVAL).on_screen_ms, 2_150);
    band.missing();
    record(&mut band, "Please wait here", 50_000);
    assert_eq!(band.stats(INTERVAL).on_screen_ms, 2_150);
}

#[test]
fn a_band_goes_stale_only_after_its_retirement_age() {
    let mut band = TrackedBand::new(1000.0, INTERVAL);
    record(&mut band, "Please wait here", 1_000);
    assert!(!band.is_stale(1_000 + WINDOW_MS, WINDOW_MS));
    assert!(band.is_stale(1_001 + WINDOW_MS, WINDOW_MS));
}

#[test]
fn failed_ocr_calls_do_not_count_as_observed_screen_time() {
    let mut band = TrackedBand::new(1000.0, INTERVAL);
    for at in [0, 250, 500, 30_000, 30_250] {
        record(&mut band, "Please wait here", at);
    }
    assert_eq!(band.stats(INTERVAL).on_screen_ms, 1_000);
}
