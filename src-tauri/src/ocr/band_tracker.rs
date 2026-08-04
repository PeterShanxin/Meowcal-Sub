// =============================================================================
// BAND_TRACKER.RS - grouping recognised lines into bands and remembering them
// =============================================================================
// `band_verdict` decides what a band is from its recent history. This is what
// assembles that history: which lines belong to the same horizontal band, what
// each frame contributed, and when to forget it again.
//
// Two things here are load-bearing and were both settled by measurement rather
// than argument.
//
// The window is bounded by *age*, not just by count. Over the measured session
// two bands were live video text for sixteen minutes and then became a frozen
// screen when playback stopped. A band is not one thing forever, so evidence
// has to expire or a band is judged on what it used to be.
//
// Horizontal position is summarised as the *smallest* scatter among the left
// edge, the centre and the right edge. Centred subtitles hold their centre;
// left-aligned ones hold their left edge while the centre wanders with line
// length. Taking the minimum covers all three alignments, and measurement
// confirmed it costs nothing: text belonging to the video scattered by 388-574
// pixels on every edge at once, because the camera moves all of them together.
// =============================================================================

use super::band_verdict::{classify, Verdict};
use super::band_window::{TrackedBand, WINDOW_MS};
use super::LineBox;

/// How long a band survives without being seen before it is forgotten.
///
/// Matches the window: once every observation has expired there is nothing left
/// to judge it on, so keeping the band would only preserve a stale verdict.
const RETIRE_MS: u64 = WINDOW_MS;

/// How close two vertical centres must be to count as the same band, as a
/// multiple of the typical line height in the frame.
///
/// Relative to line height rather than absolute so it holds at any resolution.
const BAND_TOLERANCE: f32 = 0.75;

/// Fallback line height when a frame reports no usable geometry, so grouping
/// still has a tolerance to work with rather than putting every line in its own
/// band.
const ASSUMED_LINE_HEIGHT: f32 = 32.0;

/// Lines that share a band, with where to put their translation.
#[derive(Debug, Clone, PartialEq)]
pub struct BandGroup {
    /// Vertical centre of the band in captured pixels.
    pub centre_y: f32,
    /// Indices into the `lines` slice that was observed, in the order given.
    pub lines: Vec<usize>,
}

/// A band whose lines were held back, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct DroppedBand {
    pub centre_y: f32,
    pub lines: usize,
    /// Wording from `Verdict::reason`, for the log line that reports the drop.
    pub reason: &'static str,
}

/// One frame's lines, sorted into what to translate and what to leave.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Banding {
    pub included: Vec<BandGroup>,
    pub dropped: Vec<DroppedBand>,
}

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

        let tolerance = self.tolerance(boxes);
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
            let tracked = &self.bands[band];
            let verdict = classify(&tracked.stats(self.frame_interval_ms), self.region_width);
            match verdict.reason() {
                None => banding.included.push(BandGroup {
                    centre_y: tracked.centre_y(),
                    lines,
                }),
                Some(reason) => banding.dropped.push(DroppedBand {
                    centre_y: tracked.centre_y(),
                    lines: lines.len(),
                    reason,
                }),
            }
        }

        if !ungrouped.is_empty() {
            banding.included.push(BandGroup {
                centre_y: f32::NAN,
                lines: ungrouped,
            });
        }
        banding.included.sort_by(|a, b| a.centre_y.total_cmp(&b.centre_y));
        banding
    }

    /// What this tracker currently believes about each band, for diagnostics.
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

    fn tolerance(&self, boxes: &[LineBox]) -> f32 {
        let mut heights: Vec<f32> = boxes
            .iter()
            .map(|area| area.height)
            .filter(|height| *height > 0.0)
            .collect();
        if heights.is_empty() {
            return BAND_TOLERANCE * ASSUMED_LINE_HEIGHT;
        }
        heights.sort_by(f32::total_cmp);
        BAND_TOLERANCE * heights[heights.len() / 2]
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
        let left = lines
            .iter()
            .map(|index| boxes[*index].x)
            .fold(f32::MAX, f32::min);
        let right = lines
            .iter()
            .map(|index| boxes[*index].x + boxes[*index].width)
            .fold(f32::MIN, f32::max);
        let centre_y = lines
            .iter()
            .map(|index| boxes[*index].middle_y())
            .sum::<f32>()
            / lines.len() as f32;

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
        let banding = observe(
            &mut tracker,
            vec![area(100.0, 1000.0, 200.0), area(320.0, 1006.0, 180.0)],
            0,
        );
        assert_eq!(banding.included.len(), 1);
        assert_eq!(banding.included[0].lines, vec![0, 1]);
    }

    #[test]
    fn rows_far_apart_are_separate_bands() {
        let mut tracker = BandTracker::new(REGION, INTERVAL);
        let banding = observe(
            &mut tracker,
            vec![area(100.0, 110.0, 200.0), area(100.0, 1000.0, 200.0)],
            0,
        );
        assert_eq!(banding.included.len(), 2);
        // Sorted top to bottom so a caller can place translations in order.
        assert!(banding.included[0].centre_y < banding.included[1].centre_y);
    }

    #[test]
    fn a_new_band_is_translated_before_anything_is_known_about_it() {
        let mut tracker = BandTracker::new(REGION, INTERVAL);
        let banding = observe_one(&mut tracker, 100.0, 1000.0, 200.0, 0);
        assert_eq!(banding.dropped, vec![]);
        assert_eq!(banding.included.len(), 1);
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
        assert_eq!(banding.dropped[0].reason, "unchanging");
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
        assert_eq!(banding.dropped[0].reason, "position unstable");
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
            observe_one(&mut tracker, 610.0, 1000.0, 300.0, frozen_from + frame * INTERVAL);
        }
        assert_eq!(tracker.verdicts()[0].1, Verdict::Static);
    }

    #[test]
    fn a_band_that_stops_appearing_is_forgotten() {
        let mut tracker = BandTracker::new(REGION, INTERVAL);
        play(&mut tracker, 1000.0, 40, 8, 0);
        assert_eq!(tracker.verdicts().len(), 1);
        observe_one(&mut tracker, 100.0, 110.0, 200.0, 40 * INTERVAL + RETIRE_MS + 1);
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
