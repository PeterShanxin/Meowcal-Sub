// =============================================================================
// BAND_FILTER.RS - the pipeline's one-line view of band selection
// =============================================================================
// Everything the capture loop needs to know about bands, behind a single call,
// so that `commands.rs` gains two lines rather than twenty.
//
// It owns the tracker because the tracker is stateful across frames, and it
// rebuilds it when the frame's coordinate space changes: the region can be
// reselected mid-session, and a band remembered at y=1000 in the old region
// means nothing in the new one.
// =============================================================================

use super::band_tracker::BandTracker;
use super::OcrResult;
use std::time::Instant;
use tracing::debug;

pub struct BandFilter {
    tracker: Option<BandTracker>,
    frame_interval_ms: u64,
    started: Instant,
}

impl BandFilter {
    pub fn new(frame_interval_ms: u64) -> Self {
        Self {
            tracker: None,
            frame_interval_ms: frame_interval_ms.max(1),
            started: Instant::now(),
        }
    }

    /// Keep only the lines whose band looks like it carries subtitles.
    ///
    /// A result with no geometry - an OCR path that reports text without
    /// rectangles - passes through untouched. Filtering on absent evidence
    /// would silently blank the overlay for anyone on such a path.
    pub fn apply(&mut self, result: OcrResult) -> OcrResult {
        if result.boxes.len() != result.lines.len() || result.lines.is_empty() {
            return result;
        }

        let width = result.frame_width;
        let tracker = match self.tracker.take() {
            Some(tracker) if tracker.region_width() == width => tracker,
            _ => BandTracker::new(width, self.frame_interval_ms),
        };
        self.tracker = Some(tracker);
        let tracker = self.tracker.as_mut().expect("just stored");

        let at_ms = self.started.elapsed().as_millis() as u64;
        let banding = tracker.observe(&result.lines, &result.boxes, at_ms);

        for band in &banding.dropped {
            // Glimpses are held every few seconds all session long; logging each
            // would bury the drops that explain a subtitle actually vanishing.
            if let (true, Some(reason)) = (band.verdict.is_worth_reporting(), band.verdict.reason())
            {
                debug!(
                    "[BAND: {}] held {} line(s) at y {:.0}",
                    reason, band.lines, band.centre_y
                );
            }
        }

        let mut lines = Vec::new();
        let mut boxes = Vec::new();
        for group in &banding.included {
            for index in &group.lines {
                lines.push(result.lines[*index].clone());
                boxes.push(result.boxes[*index]);
            }
        }
        OcrResult::with_boxes(lines, boxes, width)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr::LineBox;

    fn frame(y: f32, width: f32, chars: usize) -> OcrResult {
        OcrResult::with_boxes(
            vec!["x".repeat(chars)],
            vec![LineBox {
                x: 700.0,
                y,
                width,
                height: 39.0,
            }],
            1832.0,
        )
    }

    #[test]
    fn a_result_without_geometry_passes_through_untouched() {
        let mut filter = BandFilter::new(250);
        let plain = OcrResult::new(vec!["a subtitle".to_string()]);
        assert_eq!(filter.apply(plain).text, "a subtitle");
    }

    #[test]
    fn an_empty_result_stays_empty() {
        let mut filter = BandFilter::new(250);
        assert!(filter.apply(OcrResult::empty()).is_empty());
    }

    #[test]
    fn a_band_that_behaves_like_a_subtitle_survives() {
        let mut filter = BandFilter::new(250);
        let mut last = OcrResult::empty();
        // A new cue every eight frames is one every two seconds at this
        // interval, which is dialogue. Changing every frame would be credits.
        for step in 0..40 {
            let cue = step / 8 % 5;
            last = filter.apply(frame(1000.0, 300.0 + cue as f32 * 60.0, 20 + cue));
        }
        assert!(!last.is_empty(), "a subtitle band must keep being read");
    }

    // Reselecting the region changes the coordinate space, so a band remembered
    // at a height in the old one must not be applied to the new one.
    #[test]
    fn changing_the_frame_size_starts_the_bands_over() {
        let mut filter = BandFilter::new(250);
        for _ in 0..40 {
            filter.apply(frame(1000.0, 300.0, 20));
        }
        let mut narrower = frame(1000.0, 300.0, 20);
        narrower.frame_width = 900.0;
        // A brand new tracker knows nothing, so it holds the first sighting
        // back as a glimpse rather than applying the old band's verdict.
        assert!(filter.apply(narrower).is_empty());
    }
}
