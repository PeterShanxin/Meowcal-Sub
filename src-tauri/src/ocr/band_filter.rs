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
use crate::translation_eligibility::Eligibility;
use std::time::Instant;
use tracing::debug;

pub struct BandFilter {
    tracker: Option<BandTracker>,
    frame_interval_ms: u64,
    started: Instant,
    held_lines: usize,
}

impl BandFilter {
    pub fn new(frame_interval_ms: u64) -> Self {
        Self {
            tracker: None,
            frame_interval_ms: frame_interval_ms.max(1),
            started: Instant::now(),
            held_lines: 0,
        }
    }

    /// How many lines the last frame's filtering held back.
    ///
    /// The pipeline needs this to tell an empty region from one whose text was
    /// discarded here. Without it the capture loop reached its empty branch and
    /// reported `[FILTER: empty] No text detected`, telling the viewer the same,
    /// while a subtitle was plainly on screen and this filter had just thrown
    /// away 109 lines of it. `ocr_gate` exists so that text is never discarded
    /// silently; the band filter was routing around that. See issue #59.
    pub fn held_lines(&self) -> usize {
        self.held_lines
    }

    /// Why the last frame produced nothing, in the capture loop's own words.
    ///
    /// The per-band detail is already logged above as `[BAND: ...]`; this is the
    /// one word the empty branch needs so that its line agrees with them.
    pub fn skip_reason(&self) -> &'static str {
        if self.held_lines > 0 {
            "band_held"
        } else {
            "empty"
        }
    }

    /// Keep only the lines whose band looks like it carries subtitles.
    ///
    /// A result with no geometry - an OCR path that reports text without
    /// rectangles - passes through untouched. Filtering on absent evidence
    /// would silently blank the overlay for anyone on such a path.
    ///
    /// Every verdict this applies is a claim about how a band *behaves* over
    /// time, so `Eligibility::AnyText` skips it entirely rather than reading it
    /// more leniently: stationary page text is `Static` however long it is
    /// watched, and that is the case the mode exists for. The tracker goes cold
    /// while it is skipped, so switching back holds nothing until the bands are
    /// observed again - the safe direction, and the one `Verdict::Warming`
    /// already takes.
    pub fn apply(&mut self, result: OcrResult, eligibility: Eligibility) -> OcrResult {
        self.held_lines = 0;
        if !eligibility.requires_subtitle_shape() {
            return result;
        }
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

        self.held_lines = banding.dropped.iter().map(|band| band.lines).sum();
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

    const SUBTITLE: Eligibility = Eligibility::SubtitleLike;
    const ANY_TEXT: Eligibility = Eligibility::AnyText;

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
        assert_eq!(filter.apply(plain, SUBTITLE).text, "a subtitle");
    }

    #[test]
    fn an_empty_result_stays_empty() {
        let mut filter = BandFilter::new(250);
        assert!(filter.apply(OcrResult::empty(), SUBTITLE).is_empty());
    }

    #[test]
    fn a_band_that_behaves_like_a_subtitle_survives() {
        let mut filter = BandFilter::new(250);
        let mut last = OcrResult::empty();
        // A new cue every eight frames is one every two seconds at this
        // interval, which is dialogue. Changing every frame would be credits.
        for step in 0..40 {
            let cue = step / 8 % 5;
            last = filter.apply(frame(1000.0, 300.0 + cue as f32 * 60.0, 20 + cue), SUBTITLE);
        }
        assert!(!last.is_empty(), "a subtitle band must keep being read");
    }

    // The pipeline reports one of these to the viewer and writes it to the log,
    // and they mean opposite things: nothing was on screen, or something was and
    // this filter discarded it. Reporting the second as the first is why the
    // ninety-second blackout in #59 survived to 0.6.6 undiagnosed.
    #[test]
    fn a_held_frame_is_not_reported_as_an_empty_one() {
        let mut filter = BandFilter::new(250);

        filter.apply(OcrResult::empty(), SUBTITLE);
        assert_eq!(filter.held_lines(), 0);
        assert_eq!(filter.skip_reason(), "empty");

        // One sighting of a band nothing is known about is held as a glimpse -
        // the result is empty, but text was read and thrown away here.
        let held = filter.apply(frame(1000.0, 300.0, 20), SUBTITLE);
        assert!(held.is_empty(), "the frame should come back empty");
        assert_eq!(filter.held_lines(), 1);
        assert_eq!(filter.skip_reason(), "band_held");
    }

    // Reselecting the region changes the coordinate space, so a band remembered
    // at a height in the old one must not be applied to the new one.
    #[test]
    fn changing_the_frame_size_starts_the_bands_over() {
        let mut filter = BandFilter::new(250);
        for _ in 0..40 {
            filter.apply(frame(1000.0, 300.0, 20), SUBTITLE);
        }
        let mut narrower = frame(1000.0, 300.0, 20);
        narrower.frame_width = 900.0;
        // A brand new tracker knows nothing, so it holds the first sighting
        // back as a glimpse rather than applying the old band's verdict.
        assert!(filter.apply(narrower, SUBTITLE).is_empty());
    }

    // Page text, application UI, and slides hold still and hold the same words,
    // which is exactly the `Static` verdict. Subtitle-aware mode must drop it
    // and general mode must keep it, or the setting changes nothing that
    // matters.
    #[test]
    fn stationary_text_is_held_for_subtitles_and_kept_for_any_text() {
        let mut subtitles = BandFilter::new(250);
        let mut anything = BandFilter::new(250);
        let mut last_subtitles = OcrResult::empty();
        let mut last_anything = OcrResult::empty();
        for _ in 0..40 {
            last_subtitles = subtitles.apply(frame(1000.0, 300.0, 20), SUBTITLE);
            last_anything = anything.apply(frame(1000.0, 300.0, 20), ANY_TEXT);
        }
        assert!(
            last_subtitles.is_empty(),
            "unchanging text is not a subtitle"
        );
        assert!(subtitles.held_lines() > 0);
        assert!(!last_anything.is_empty(), "general text must survive");
        assert_eq!(anything.held_lines(), 0);
        assert_eq!(anything.skip_reason(), "empty");
    }

    // Nothing here invents text: an empty frame is still empty, whatever the
    // pipeline has been asked to translate.
    #[test]
    fn any_text_mode_still_reports_an_empty_frame_as_empty() {
        let mut filter = BandFilter::new(250);
        assert!(filter.apply(OcrResult::empty(), ANY_TEXT).is_empty());
        assert_eq!(filter.held_lines(), 0);
        assert_eq!(filter.skip_reason(), "empty");
    }
}
