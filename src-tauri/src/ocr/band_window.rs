// =============================================================================
// BAND_WINDOW.RS - what one band remembers about itself
// =============================================================================
// `band_tracker` decides which lines belong to which band; this is what a band
// keeps once they do, and the arithmetic that turns it into the figures
// `band_verdict` judges.
//
// The window is bounded by *age*, not just by count. Over the measured session
// two bands were live video text for sixteen minutes and then became a frozen
// screen when playback stopped. A band is not one thing forever, so its
// evidence has to expire or it goes on being judged on what it used to be.
//
// Horizontal position is summarised as the *smallest* scatter among the left
// edge, the centre and the right edge. Centred subtitles hold their centre;
// left-aligned ones hold their left edge while the centre wanders with line
// length. Taking the minimum covers every alignment, and measurement confirmed
// it costs nothing: text belonging to the video scattered by 388-574 pixels on
// all three edges at once, because the camera moves them together.
// =============================================================================

use super::band_verdict::{is_same_cue, BandStats};
use std::collections::VecDeque;

/// How long a band's evidence stays relevant.
pub(super) const WINDOW_MS: u64 = 90_000;

#[derive(Debug, Clone, Copy)]
struct Observation {
    at_ms: u64,
    left: f32,
    right: f32,
    chars: usize,
}

impl Observation {
    fn centre(&self) -> f32 {
        (self.left + self.right) / 2.0
    }

    fn width(&self) -> f32 {
        self.right - self.left
    }
}

/// One band's recent history.
#[derive(Debug)]
pub(super) struct TrackedBand {
    centre_y: f32,
    last_seen_ms: u64,
    window: VecDeque<Observation>,
}

impl TrackedBand {
    pub(super) fn new(centre_y: f32) -> Self {
        Self {
            centre_y,
            last_seen_ms: 0,
            window: VecDeque::new(),
        }
    }

    pub(super) fn centre_y(&self) -> f32 {
        self.centre_y
    }

    /// Whether every observation has expired, leaving nothing to judge on.
    pub(super) fn is_stale(&self, at_ms: u64, retire_ms: u64) -> bool {
        at_ms.saturating_sub(self.last_seen_ms) > retire_ms
    }

    /// Add what one frame contributed and drop whatever has aged out.
    pub(super) fn record(
        &mut self,
        centre_y: f32,
        left: f32,
        right: f32,
        chars: usize,
        at_ms: u64,
    ) {
        // Drift with the band rather than pinning it where it was first seen:
        // subtitles shift by a few pixels between cues.
        self.centre_y = self.centre_y * 0.9 + centre_y * 0.1;
        self.last_seen_ms = at_ms;
        self.window.push_back(Observation {
            at_ms,
            left,
            right,
            chars,
        });
        while self
            .window
            .front()
            .is_some_and(|oldest| at_ms.saturating_sub(oldest.at_ms) > WINDOW_MS)
        {
            self.window.pop_front();
        }
    }

    pub(super) fn stats(&self, frame_interval_ms: u64) -> BandStats {
        let observations: Vec<&Observation> = self.window.iter().collect();
        let lefts: Vec<f32> = observations.iter().map(|o| o.left).collect();
        let centres: Vec<f32> = observations.iter().map(|o| o.centre()).collect();
        let rights: Vec<f32> = observations.iter().map(|o| o.right).collect();

        let mut cues = usize::from(!observations.is_empty());
        for pair in observations.windows(2) {
            if !is_same_cue(
                pair[1].width(),
                pair[1].chars,
                pair[0].width(),
                pair[0].chars,
            ) {
                cues += 1;
            }
        }

        BandStats {
            observations: observations.len(),
            centre_scatter: scatter(&lefts).min(scatter(&centres)).min(scatter(&rights)),
            cues,
            on_screen_ms: observations.len() as u64 * frame_interval_ms,
        }
    }
}

/// Standard deviation, or zero for fewer than two samples.
fn scatter(values: &[f32]) -> f32 {
    if values.len() < 2 {
        return 0.0;
    }
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    (values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
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
            band.record(1000.0, 600.0, right, 20 + frame as usize * 12, frame * INTERVAL);
        }
        assert_eq!(band.stats(INTERVAL).cues, 6);
    }

    #[test]
    fn a_band_goes_stale_only_after_its_retirement_age() {
        let mut band = TrackedBand::new(1000.0);
        band.record(1000.0, 600.0, 900.0, 20, 1_000);
        assert!(!band.is_stale(1_000 + WINDOW_MS, WINDOW_MS));
        assert!(band.is_stale(1_001 + WINDOW_MS + 1, WINDOW_MS));
    }
}
