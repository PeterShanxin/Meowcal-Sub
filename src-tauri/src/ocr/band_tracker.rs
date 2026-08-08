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
#[cfg(test)]
use super::band_window::{MAX_DEMOTION_MS, READMITTED_GRACE_FRAMES};
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
            // Before recording, not after: expiry clears the window, and doing
            // it second would throw away the observation this frame just added -
            // leaving the band at zero and costing it a further frame before it
            // can be judged. See `MAX_DEMOTION_MS`.
            self.bands[band].expire_a_stale_demotion(at_ms);
            self.record(band, chars, boxes, &lines, at_ms);
            let (interval, width) = (self.frame_interval_ms, self.region_width);
            let tracked = &mut self.bands[band];
            let raw = classify(&tracked.stats(interval), width);
            let verdict = tracked.settle(raw, at_ms);
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
#[path = "band_tracker_tests.rs"]
mod tests;
