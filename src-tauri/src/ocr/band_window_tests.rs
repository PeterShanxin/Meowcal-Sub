use super::*;

const INTERVAL: u64 = 261;

#[test]
fn observations_older_than_the_window_are_forgotten() {
    let mut band = TrackedBand::new(1000.0);
    band.record(1000.0, 600.0, 900.0, 20, 0);
    band.record(1000.0, 600.0, 900.0, 20, WINDOW_MS / 2);
    assert_eq!(band.stats(INTERVAL).observations, 2);
    band.record(1000.0, 600.0, 900.0, 20, WINDOW_MS + 1);
    assert_eq!(
        band.stats(INTERVAL).observations,
        2,
        "the first observation should have aged out"
    );
}

#[test]
fn a_band_drifts_toward_where_it_is_actually_seen() {
    let mut band = TrackedBand::new(1000.0);
    for _ in 0..40 {
        band.record(1010.0, 600.0, 900.0, 20, 0);
    }
    assert!(band.centre_y() > 1008.0, "{}", band.centre_y());
    assert!(band.centre_y() <= 1010.0);
}

// A left-aligned band holds its left edge while its centre and right edge
// move with the length of each line. The smallest of the three is what
// counts, so this must read as holding position.
#[test]
fn a_left_aligned_band_is_judged_on_its_left_edge() {
    let mut band = TrackedBand::new(1000.0);
    for (index, right) in [700.0, 1100.0, 850.0, 1300.0, 900.0].iter().enumerate() {
        band.record(1000.0, 300.0, *right, 20, index as u64 * INTERVAL);
    }
    assert_eq!(band.stats(INTERVAL).centre_scatter, 0.0);
}

#[test]
fn a_band_seen_once_has_no_scatter_to_speak_of() {
    let mut band = TrackedBand::new(1000.0);
    band.record(1000.0, 600.0, 900.0, 20, 0);
    let stats = band.stats(INTERVAL);
    assert_eq!(stats.centre_scatter, 0.0);
    assert_eq!(stats.cues, 1);
}

#[test]
fn holding_the_same_reading_counts_as_one_cue() {
    let mut band = TrackedBand::new(1000.0);
    for frame in 0..20u64 {
        band.record(1000.0, 600.0, 900.0, 20, frame * INTERVAL);
    }
    assert_eq!(band.stats(INTERVAL).cues, 1);
}

#[test]
fn each_genuinely_new_reading_is_another_cue() {
    let mut band = TrackedBand::new(1000.0);
    for frame in 0..6u64 {
        let right = 900.0 + frame as f32 * 200.0;
        band.record(
            1000.0,
            600.0,
            right,
            20 + frame as usize * 12,
            frame * INTERVAL,
        );
    }
    assert_eq!(band.stats(INTERVAL).cues, 6);
}

// A subtitle band that reads as something else for a frame or two must keep
// being translated. That flicker is what makes a short line fail to appear.
#[test]
fn an_established_subtitle_band_survives_a_brief_disagreement() {
    let mut band = TrackedBand::new(1000.0);
    assert_eq!(band.settle(Verdict::Subtitle, 0), Verdict::Subtitle);
    for frame in 0..HOLD_THROUGH_FRAMES - 1 {
        assert_eq!(
            band.settle(Verdict::Churning, 0),
            Verdict::Subtitle,
            "frame {frame} should still be translated"
        );
    }
}

#[test]
fn a_sustained_disagreement_does_demote_it() {
    let mut band = TrackedBand::new(1000.0);
    band.settle(Verdict::Subtitle, 0);
    for _ in 0..HOLD_THROUGH_FRAMES {
        band.settle(Verdict::Churning, 0);
    }
    assert_eq!(band.settle(Verdict::Churning, 0), Verdict::Churning);
}

// The protection is for a band that has earned it, not for anything that
// happens to be included at the time.
#[test]
fn a_band_that_was_never_a_subtitle_is_demoted_at_once() {
    let mut band = TrackedBand::new(1000.0);
    band.settle(Verdict::Warming, 0);
    assert_eq!(band.settle(Verdict::Scattered, 0), Verdict::Scattered);
}

// Recognition is immediate in the other direction, or a subtitle waits
// eight frames to appear - the very failure this exists to prevent.
#[test]
fn becoming_a_subtitle_takes_effect_immediately() {
    let mut band = TrackedBand::new(1000.0);
    band.settle(Verdict::Glimpsed, 0);
    assert_eq!(band.settle(Verdict::Subtitle, 0), Verdict::Subtitle);
}

// A disagreement that does not persist must not accumulate towards a later
// demotion, or enough scattered single frames eventually add up to one.
#[test]
fn a_recovered_band_starts_its_grace_period_over() {
    let mut band = TrackedBand::new(1000.0);
    band.settle(Verdict::Subtitle, 0);
    for _ in 0..HOLD_THROUGH_FRAMES - 1 {
        band.settle(Verdict::Churning, 0);
    }
    band.settle(Verdict::Subtitle, 0);
    assert_eq!(band.settle(Verdict::Churning, 0), Verdict::Subtitle);
}

#[test]
fn a_band_goes_stale_only_after_its_retirement_age() {
    let mut band = TrackedBand::new(1000.0);
    band.record(1000.0, 600.0, 900.0, 20, 1_000);
    assert!(!band.is_stale(1_000 + WINDOW_MS, WINDOW_MS));
    assert!(band.is_stale(1_001 + WINDOW_MS + 1, WINDOW_MS));
}
