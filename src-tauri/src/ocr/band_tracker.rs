// =============================================================================
// BAND_TRACKER.RS - grouping recognised lines into bands and remembering them
// =============================================================================
// `band_verdict` decides what a band is from its recent history. This is what
// assembles that history: which lines belong to the same horizontal band, what
// each frame contributed, and when to forget it again.
//
// What a band remembers, and why its evidence expires, is in `band_window`.
// How a frame's rectangles become one band's position is in `band_geometry`.
// =============================================================================

use super::band_verdict::classify;
// Only `verdicts` and the tests name the verdict type; the pipeline acts on
// what `observe` hands back.
#[cfg(test)]
use super::band_verdict::Verdict;
use super::band_window::{TrackedBand, WINDOW_MS};
use super::banding::{BandGroup, Banding, DroppedBand};
use super::LineBox;

/// How long a band survives without being seen before it is forgotten.
///
/// Matches the window: once every observation has expired there is nothing left
/// to judge it on, so keeping the band would only preserve a stale verdict.
const RETIRE_MS: u64 = WINDOW_MS;

/// Remembers what each horizontal band has been doing.
///
/// `at_ms` is supplied by the caller rather than read from a clock here, so the
/// tests can drive a session in milliseconds instead of sleeping through one.
#[derive(Debug)]
pub struct BandTracker {
    region_width: f32,
    frame_interval_ms: u64,
    bands: Vec<TrackedBand>,
}

impl BandTracker {
    pub fn new(region_width: f32, frame_interval_ms: u64) -> Self {
        Self {
            region_width,
            frame_interval_ms: frame_interval_ms.max(1),
            bands: Vec::new(),
        }
    }

    /// Take one frame's recognition and say which lines to translate.
    ///
    /// A line with no usable geometry - `line_geometry` emits a zero rectangle
    /// when Windows will not say where a line was - is always included and never
    /// assigned to a band. Guessing a band for it would corrupt that band's
    /// history, and dropping it would lose a subtitle for a reason the user
    /// cannot see.
    pub fn observe(&mut self, texts: &[String], boxes: &[LineBox], at_ms: u64) -> Banding {
        self.retire(at_ms);

        let tolerance = super::band_geometry::tolerance(boxes);
        let mut per_band: Vec<(usize, Vec<usize>)> = Vec::new();
        let mut ungrouped: Vec<usize> = Vec::new();

        for (index, area) in boxes.iter().enumerate() {
            if area.height <= 0.0 || area.width <= 0.0 {
                ungrouped.push(index);
                continue;
            }
            let band = self.band_for(area.middle_y(), tolerance);
            match per_band.iter_mut().find(|(id, _)| *id == band) {
                Some((_, lines)) => lines.push(index),
                None => per_band.push((band, vec![index])),
            }
        }

        let mut banding = Banding::default();
        for (band, lines) in per_band {
            let chars = lines
                .iter()
                .filter_map(|index| texts.get(*index))
                .map(|text| text.chars().count())
                .sum();
            self.record(band, chars, boxes, &lines, at_ms);
            let (interval, width) = (self.frame_interval_ms, self.region_width);
            let tracked = &mut self.bands[band];
            let raw = classify(&tracked.stats(interval), width);
            let verdict = tracked.settle(raw);
            let tracked = &self.bands[band];
            if verdict.is_included() {
                banding.included.push(BandGroup {
                    centre_y: tracked.centre_y(),
                    lines,
                });
            } else {
                banding.dropped.push(DroppedBand {
                    centre_y: tracked.centre_y(),
                    lines: lines.len(),
                    verdict,
                });
            }
        }

        if !ungrouped.is_empty() {
            banding.included.push(BandGroup {
                centre_y: f32::NAN,
                lines: ungrouped,
            });
        }
        banding
            .included
            .sort_by(|a, b| a.centre_y.total_cmp(&b.centre_y));
        banding
    }

    /// The frame width these bands were measured against. A change means the
    /// region was reselected and the remembered heights no longer refer to
    /// anything.
    pub fn region_width(&self) -> f32 {
        self.region_width
    }

    /// What this tracker currently believes about each band.
    ///
    /// Only the tests and `band_replay` ask - the pipeline acts on the verdict
    /// it is handed per frame rather than interrogating the tracker - so this
    /// is not compiled into the app.
    #[cfg(test)]
    pub fn verdicts(&self) -> Vec<(f32, Verdict)> {
        self.bands
            .iter()
            .map(|band| {
                (
                    band.centre_y(),
                    classify(&band.stats(self.frame_interval_ms), self.region_width),
                )
            })
            .collect()
    }

    fn band_for(&mut self, centre_y: f32, tolerance: f32) -> usize {
        let nearest = self
            .bands
            .iter()
            .enumerate()
            .map(|(index, band)| (index, (band.centre_y() - centre_y).abs()))
            .filter(|(_, distance)| *distance <= tolerance)
            .min_by(|a, b| a.1.total_cmp(&b.1));

        match nearest {
            Some((index, _)) => index,
            None => {
                self.bands.push(TrackedBand::new(centre_y));
                self.bands.len() - 1
            }
        }
    }

    fn record(
        &mut self,
        band: usize,
        chars: usize,
        boxes: &[LineBox],
        lines: &[usize],
        at_ms: u64,
    ) {
        let (left, right, centre_y) = super::band_geometry::union(boxes, lines);
        self.bands[band].record(centre_y, left, right, chars, at_ms);
    }

    fn retire(&mut self, at_ms: u64) {
        self.bands.retain(|band| !band.is_stale(at_ms, RETIRE_MS));
    }
}

#[cfg(test)]
mod tests {
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

    /// A line of plausible length for a box that wide.
    ///
    /// Derived from the width so the two halves of `is_same_cue` stay
    /// consistent, but not equal to it - the character count has to carry its
    /// own signal rather than echo the geometry.
    fn text_for(width: f32) -> String {
        "x".repeat((width / 14.0) as usize)
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
    fn a_band_is_translated_once_seen_a_few_times_not_on_first_sight() {
        let mut tracker = BandTracker::new(REGION, INTERVAL);
        let first = observe_one(&mut tracker, 100.0, 1000.0, 200.0, 0);
        assert_eq!(first.included, vec![]);
        assert_eq!(first.dropped[0].verdict, Verdict::Glimpsed);

        observe_one(&mut tracker, 100.0, 1000.0, 200.0, INTERVAL);
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
        let banding = observe_one(&mut tracker, 600.0, 1000.0, 320.0, 120 * INTERVAL);
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
        let banding = observe(
            &mut tracker,
            vec![
                area(760.0 - width / 2.0, 110.0, width),
                area(760.0 - width / 2.0, 1000.0, width),
            ],
            120 * INTERVAL,
        );
        assert_eq!(banding.dropped, vec![]);
        assert_eq!(banding.included.len(), 2);
    }
}
