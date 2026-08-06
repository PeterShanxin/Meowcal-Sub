// =============================================================================
// PIPELINE_NOTICES.RS - deciding when to tell the viewer why nothing is showing
// =============================================================================
// A frame that produces no translation still owes the viewer an explanation:
// the region is empty, the text was unreadable, the band was held. What it must
// not do is repeat itself. The capture loop runs four times a second, and an
// unchanged notice re-emitted on every frame overwrites whatever is on screen
// and makes the overlay flicker.
//
// So each notice is emitted once, when it changes. That is two pieces of state -
// which notice is showing, and how many empty frames have passed - and they were
// two bare locals in a two-thousand-line loop, read and written in three places.
// Keeping them together makes the rule "only when it changes" checkable in one
// place instead of three.
// =============================================================================

use crate::event_payloads::{TranslationPayload, EMPTY_OCR_CLEAR_FRAMES};

/// What the viewer was last told about a frame that produced no translation.
#[derive(Debug, Default)]
pub struct Notices {
    showing: Option<&'static str>,
    empty_frames: u32,
}

impl Notices {
    pub fn new() -> Self {
        Self::default()
    }

    /// A frame whose region held no text worth translating.
    ///
    /// `Some` only when the viewer needs telling: the region has been quiet long
    /// enough to mean it, nothing else is in flight, and this is not the notice
    /// already on screen. Waiting a few frames keeps a single-frame OCR miss
    /// from flickering the line away between cues.
    pub(crate) fn quiet_region(
        &mut self,
        session_id: u64,
        capture_id: u64,
        held_lines: usize,
        busy: bool,
    ) -> Option<TranslationPayload> {
        self.empty_frames = self.empty_frames.saturating_add(1);
        let settled = self.empty_frames >= EMPTY_OCR_CLEAR_FRAMES;
        if !settled || busy || self.showing == Some("empty") {
            return None;
        }
        self.showing = Some("empty");
        Some(TranslationPayload::region_quiet(
            session_id, capture_id, held_lines,
        ))
    }

    /// A frame that held text the gate refused.
    ///
    /// Text *is* in the region, so a notice claiming otherwise has to be
    /// replaced - staying silent leaves whatever is on screen contradicting what
    /// the viewer can see.
    pub(crate) fn unreadable_source(
        &mut self,
        session_id: u64,
        capture_id: u64,
        rejection: crate::ocr_gate::OcrRejection,
        busy: bool,
    ) -> Option<TranslationPayload> {
        if busy || self.showing == Some(rejection.as_str()) {
            return None;
        }
        self.showing = Some(rejection.as_str());
        Some(TranslationPayload::source_unreadable(
            session_id, capture_id, rejection,
        ))
    }

    /// A frame that produced a translation clears the count and the notice.
    pub fn translated(&mut self) {
        self.empty_frames = 0;
        self.showing = None;
    }

    /// A frame that read text, whether or not it was translated.
    pub fn saw_text(&mut self) {
        self.empty_frames = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr_gate::OcrRejection;

    fn quiet(notices: &mut Notices) -> Option<TranslationPayload> {
        notices.quiet_region(1, 1, 0, false)
    }

    // The overlay is redrawn four times a second. Repeating an unchanged notice
    // overwrites whatever is on screen and makes it flicker.
    #[test]
    fn a_notice_is_sent_once_rather_than_every_frame() {
        let mut notices = Notices::new();
        let mut sent = 0;
        for _ in 0..20 {
            if quiet(&mut notices).is_some() {
                sent += 1;
            }
        }
        assert_eq!(sent, 1);
    }

    // A single-frame OCR miss between two cues must not blank the line.
    #[test]
    fn one_empty_frame_is_not_enough_to_clear_the_overlay() {
        let mut notices = Notices::new();
        assert!(quiet(&mut notices).is_none());
    }

    // Text is in the region, so a notice saying it is empty has to be replaced.
    #[test]
    fn a_different_notice_replaces_the_one_on_screen() {
        let mut notices = Notices::new();
        for _ in 0..EMPTY_OCR_CLEAR_FRAMES {
            quiet(&mut notices);
        }
        assert!(notices
            .unreadable_source(1, 2, OcrRejection::TooShort, false)
            .is_some());
        // ...but not twice.
        assert!(notices
            .unreadable_source(1, 3, OcrRejection::TooShort, false)
            .is_none());
    }

    // A translation in flight will speak for itself; a notice now would race it.
    #[test]
    fn nothing_is_said_while_a_translation_is_in_flight() {
        let mut notices = Notices::new();
        for _ in 0..20 {
            assert!(notices.quiet_region(1, 1, 0, true).is_none());
        }
    }

    // A line that translated means the region is live again, so the next quiet
    // stretch has to be reported from scratch.
    #[test]
    fn a_translated_line_resets_the_notice() {
        let mut notices = Notices::new();
        for _ in 0..EMPTY_OCR_CLEAR_FRAMES {
            quiet(&mut notices);
        }
        notices.translated();

        let mut sent = 0;
        for _ in 0..20 {
            if quiet(&mut notices).is_some() {
                sent += 1;
            }
        }
        assert_eq!(
            sent, 1,
            "the notice should be sent again after a translation"
        );
    }
}
