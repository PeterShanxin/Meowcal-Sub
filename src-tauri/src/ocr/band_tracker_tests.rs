use super::*;

const REGION: f32 = 1832.0;
const INTERVAL: u64 = 261;

fn area(x: f32, y: f32, width: f32) -> LineBox {
    LineBox {
        x,
        y,
        width,
        height: 39.0,
    }
}

fn text_for(width: f32) -> String {
    let cues = [
        "Please wait by the entrance.",
        "I thought you had already left.",
        "Where were you last night?",
        "We should talk about this tomorrow.",
        "The trains had stopped.",
    ];
    cues[(width as usize / 60) % cues.len()].to_owned()
}
fn observe(tracker: &mut BandTracker, boxes: Vec<LineBox>, at_ms: u64) -> Banding {
    let texts: Vec<String> = boxes.iter().map(|area| text_for(area.width)).collect();
    tracker.observe(&texts, &boxes, at_ms)
}

fn observe_one(tracker: &mut BandTracker, x: f32, y: f32, width: f32, at_ms: u64) -> Banding {
    observe(tracker, vec![area(x, y, width)], at_ms)
}

/// Drive `frames` frames of a band that holds position and changes cue
/// every `cue_every` frames, which is what a subtitle does.
fn play(tracker: &mut BandTracker, y: f32, frames: usize, cue_every: usize, from_ms: u64) {
    for frame in 0..frames {
        let width = 300.0 + ((frame / cue_every) % 5) as f32 * 60.0;
        let at = from_ms + frame as u64 * INTERVAL;
        observe_one(tracker, 760.0 - width / 2.0, y, width, at);
    }
}

#[test]
fn lines_on_the_same_row_share_a_band() {
    let mut tracker = BandTracker::new(REGION, INTERVAL);
    let rows = vec![area(100.0, 1000.0, 200.0), area(320.0, 1006.0, 180.0)];
    let mut banding = Banding::default();
    for frame in 0..4u64 {
        banding = observe(&mut tracker, rows.clone(), frame * INTERVAL);
    }
    assert_eq!(banding.included.len(), 1);
    assert_eq!(banding.included[0].lines, vec![0, 1]);
}

#[test]
fn rows_far_apart_are_separate_bands() {
    let mut tracker = BandTracker::new(REGION, INTERVAL);
    let rows = vec![area(100.0, 110.0, 200.0), area(100.0, 1000.0, 200.0)];
    let mut banding = Banding::default();
    for frame in 0..4u64 {
        banding = observe(&mut tracker, rows.clone(), frame * INTERVAL);
    }
    assert_eq!(banding.included.len(), 2);
    // Sorted top to bottom so a caller can place translations in order.
    assert!(banding.included[0].centre_y < banding.included[1].centre_y);
}

// Default-open, but only once a band has been seen enough to be a band at
// all. One sighting is a stray recognition off the video; by the third the
// benefit of the doubt applies, long before there is enough history to
// judge what kind of band it is.
#[test]
fn a_band_is_translated_after_two_agreeing_readings_not_on_first_sight() {
    let mut tracker = BandTracker::new(REGION, INTERVAL);
    let first = observe_one(&mut tracker, 100.0, 1000.0, 200.0, 0);
    assert_eq!(first.included, vec![]);
    assert_eq!(first.dropped[0].verdict, Verdict::Glimpsed);

    let second = observe_one(&mut tracker, 100.0, 1000.0, 200.0, INTERVAL);
    assert_eq!(second.included.len(), 1);
    let third = observe_one(&mut tracker, 100.0, 1000.0, 200.0, 2 * INTERVAL);
    assert_eq!(third.dropped, vec![], "three sightings is a band");
    assert_eq!(third.included.len(), 1);
}

// A glimpse is still reported to the caller - nothing is discarded
// silently - it is only marked as not worth a log line of its own.
#[test]
fn a_glimpse_is_reported_but_not_worth_logging() {
    let mut tracker = BandTracker::new(REGION, INTERVAL);
    let banding = observe_one(&mut tracker, 100.0, 1000.0, 200.0, 0);
    assert_eq!(banding.dropped.len(), 1);
    assert_eq!(banding.dropped[0].lines, 1);
    assert!(!banding.dropped[0].verdict.is_worth_reporting());
}

#[test]
fn a_band_that_holds_still_and_keeps_changing_is_kept() {
    let mut tracker = BandTracker::new(REGION, INTERVAL);
    play(&mut tracker, 1000.0, 120, 8, 0);
    observe_one(&mut tracker, 600.0, 1000.0, 320.0, 120 * INTERVAL);
    let banding = observe_one(&mut tracker, 600.0, 1000.0, 320.0, 121 * INTERVAL);
    assert_eq!(banding.dropped, vec![], "a subtitle band must survive");
    assert_eq!(banding.included.len(), 1);
}

#[test]
fn a_band_that_never_changes_is_dropped_as_unchanging() {
    let mut tracker = BandTracker::new(REGION, INTERVAL);
    for frame in 0..120u64 {
        observe_one(&mut tracker, 600.0, 574.0, 320.0, frame * INTERVAL);
    }
    let banding = observe_one(&mut tracker, 600.0, 574.0, 320.0, 120 * INTERVAL);
    assert_eq!(banding.included, vec![]);
    assert_eq!(banding.dropped.len(), 1);
    assert_eq!(banding.dropped[0].verdict, Verdict::Static);
}

#[test]
fn a_band_that_will_not_hold_position_is_dropped() {
    let mut tracker = BandTracker::new(REGION, INTERVAL);
    for frame in 0..120u64 {
        // Sweeping across the frame the way text in the video does.
        let x = ((frame * 37) % 1400) as f32;
        observe_one(
            &mut tracker,
            x,
            700.0,
            200.0 + (frame % 7) as f32 * 30.0,
            frame * INTERVAL,
        );
    }
    let banding = observe_one(&mut tracker, 400.0, 700.0, 260.0, 120 * INTERVAL);
    assert_eq!(banding.included, vec![]);
    assert_eq!(banding.dropped[0].verdict, Verdict::Scattered);
}

// The measured session's central finding: a band changes character when
// playback stops. Evidence has to expire or the old verdict outlives it.
#[test]
fn a_band_is_rejudged_once_its_old_evidence_expires() {
    let mut tracker = BandTracker::new(REGION, INTERVAL);
    play(&mut tracker, 1000.0, 200, 8, 0);
    assert!(tracker.verdicts()[0].1 == Verdict::Subtitle);

    // Now the same band goes static for longer than the window.
    let frozen_from = 200 * INTERVAL;
    for frame in 0..500u64 {
        observe_one(
            &mut tracker,
            610.0,
            1000.0,
            300.0,
            frozen_from + frame * INTERVAL,
        );
    }
    assert_eq!(tracker.verdicts()[0].1, Verdict::Static);
}

#[test]
fn a_band_that_stops_appearing_is_forgotten() {
    let mut tracker = BandTracker::new(REGION, INTERVAL);
    play(&mut tracker, 1000.0, 40, 8, 0);
    assert_eq!(tracker.verdicts().len(), 1);
    observe_one(
        &mut tracker,
        100.0,
        110.0,
        200.0,
        40 * INTERVAL + RETIRE_MS + 1,
    );
    let remaining = tracker.verdicts();
    assert_eq!(remaining.len(), 1, "the stale band should be gone");
    assert!(remaining[0].0 < 200.0, "only the new band should remain");
}

// Losing a subtitle because Windows would not say where it was is worse
// than not knowing where it was, so it is translated and never banded.
#[test]
fn a_line_with_no_geometry_is_still_translated() {
    let mut tracker = BandTracker::new(REGION, INTERVAL);
    let banding = observe(
        &mut tracker,
        vec![LineBox {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        }],
        0,
    );
    assert_eq!(banding.dropped, vec![]);
    assert_eq!(banding.included.len(), 1);
    assert_eq!(banding.included[0].lines, vec![0]);
    assert!(tracker.verdicts().is_empty(), "it must not pollute a band");
}

// A top band and a bottom band being live at once was 2.9% of the measured
// frames. Both have to survive; picking one is what loses sign translations.
#[test]
fn two_live_bands_are_both_kept() {
    let mut tracker = BandTracker::new(REGION, INTERVAL);
    for frame in 0..120u64 {
        let width = 300.0 + (frame / 8 % 5) as f32 * 60.0;
        observe(
            &mut tracker,
            vec![
                area(760.0 - width / 2.0, 110.0, width),
                area(760.0 - width / 2.0, 1000.0, width),
            ],
            frame * INTERVAL,
        );
    }
    let width = 340.0;
    observe(
        &mut tracker,
        vec![
            area(760.0 - width / 2.0, 110.0, width),
            area(760.0 - width / 2.0, 1000.0, width),
        ],
        120 * INTERVAL,
    );
    let banding = observe(
        &mut tracker,
        vec![
            area(760.0 - width / 2.0, 110.0, width),
            area(760.0 - width / 2.0, 1000.0, width),
        ],
        121 * INTERVAL,
    );
    assert_eq!(banding.dropped, vec![]);
    assert_eq!(banding.included.len(), 2);
}
