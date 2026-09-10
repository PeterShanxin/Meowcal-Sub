//! Which OCR text a running session is willing to translate.
//!
//! The policy owns itself because three rules in three modules consult it, and
//! none of them should decide for the others.

/// Which recognised text this session is willing to translate.
///
/// Three rules in the pipeline ask whether text *looks like a subtitle* rather
/// than whether it can be translated at all:
///
/// - `ocr::BandFilter`, which judges a band of text by how it moves and how
///   often it turns over - page text holds still and reads as `Static`;
/// - the cue length limit in `ocr_gate`, sized for one or two lines of dialogue;
/// - the cue length cap in `llm::output_validation`, sized for what a cue's
///   translation can come to.
///
/// All three are right for a video and wrong for a web page, a game menu, or a
/// slide, so the viewer chooses which question the pipeline asks.
///
/// Nothing else changes. Empty text, OCR noise, unchanged lines, pacing, the
/// single translation slot, and the disproportion guard on generated output are
/// engineering limits rather than judgements about what the text is, and they
/// apply in both modes.
///
/// Read once per translation session, alongside every other translation
/// setting, so the three rules can never disagree within one session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Eligibility {
    /// Only text that behaves like a subtitle track.
    #[default]
    SubtitleLike,
    /// Any readable line inside the capture region.
    AnyText,
}

impl Eligibility {
    pub fn from_translate_all(translate_all_ocr_text: bool) -> Self {
        if translate_all_ocr_text {
            Eligibility::AnyText
        } else {
            Eligibility::SubtitleLike
        }
    }

    /// Whether a line still has to look like a subtitle cue to be translated.
    pub fn requires_subtitle_shape(self) -> bool {
        matches!(self, Eligibility::SubtitleLike)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_policy_is_the_subtitle_aware_one() {
        assert_eq!(Eligibility::default(), Eligibility::SubtitleLike);
        assert_eq!(
            Eligibility::from_translate_all(false),
            Eligibility::SubtitleLike
        );
        assert_eq!(Eligibility::from_translate_all(true), Eligibility::AnyText);
        assert!(Eligibility::SubtitleLike.requires_subtitle_shape());
        assert!(!Eligibility::AnyText.requires_subtitle_shape());
    }
}
