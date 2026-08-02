// =============================================================================
// OCR GATE - why a recognised line never reaches the translator
// =============================================================================
// OCR returning text is not the same as OCR returning *usable* text. The capture
// loop drops lines that are not worth translating, and until this module existed
// it dropped them silently: the overlay kept whatever notice was already on
// screen, which made an unreadable region indistinguishable from an empty one.
//
// Keeping the decision here (instead of inline in the capture loop) means the
// reason is a value the pipeline can report to the viewer, and that every rule
// is unit-testable without a screen, a webview, or Windows OCR.
// =============================================================================

/// Why a recognised OCR line was not worth translating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrRejection {
    /// Too few alphanumeric characters - specks, borders, or subtitle edges
    /// caught mid-fade rather than a line of dialogue.
    TooShort,
    /// Real characters, but nothing a translator can act on (digits, symbols,
    /// timestamps).
    Untranslatable,
}

impl OcrRejection {
    /// Stable identifier shared with the frontend, which owns the wording.
    pub fn as_str(self) -> &'static str {
        match self {
            OcrRejection::TooShort => "tooShort",
            OcrRejection::Untranslatable => "untranslatable",
        }
    }
}

/// Decide whether a recognised line should be translated.
///
/// `None` means the line is good enough to send onward.
///
/// There is deliberately no confidence check. Windows OCR publishes no
/// confidence of any kind, and the score that used to stand in for one was
/// computed from the shape of the recognised text - character variety, length,
/// punctuation density - so it scored correct, legible subtitles below the
/// threshold for the crime of being short or unpunctuated, and multiplied them
/// down again for containing an ellipsis. Every rule here is a claim about the
/// text that the text itself can support.
pub fn classify(text: &str, min_significant_chars: usize) -> Option<OcrRejection> {
    let significant_chars = text.chars().filter(|ch| ch.is_alphanumeric()).count();
    if significant_chars < min_significant_chars {
        return Some(OcrRejection::TooShort);
    }

    if crate::llm::is_untranslatable_text(text) {
        return Some(OcrRejection::Untranslatable);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODERATE: usize = 2;

    #[test]
    fn accepts_a_line_of_dialogue() {
        assert_eq!(classify("Where are you going?", MODERATE), None);
    }

    // The regression that motivated this rewrite: a perfectly recognised
    // subtitle was discarded for being short and unpunctuated.
    #[test]
    fn accepts_a_short_unpunctuated_subtitle() {
        assert_eq!(classify("Welcome to Meowcal Sub", MODERATE), None);
        assert_eq!(classify("Hi", MODERATE), None);
    }

    // Ellipses and dashes are ordinary subtitle punctuation, and the old
    // heuristic multiplied its score down for containing them.
    #[test]
    fn accepts_dialogue_full_of_ellipses_and_dashes() {
        assert_eq!(classify("I do not know... maybe --- later", MODERATE), None);
    }

    #[test]
    fn rejects_a_single_alphanumeric_character() {
        assert_eq!(classify("a.", MODERATE), Some(OcrRejection::TooShort));
    }

    #[test]
    fn rejects_punctuation_only_noise() {
        assert_eq!(classify("--- ...", MODERATE), Some(OcrRejection::TooShort));
    }

    #[test]
    fn honours_the_configured_minimum() {
        assert_eq!(classify("Hi", MODERATE), None);
        assert_eq!(classify("Hi", 3), Some(OcrRejection::TooShort));
    }

    #[test]
    fn counts_cjk_glyphs_as_significant() {
        // A two-character Chinese line is a complete subtitle, not noise.
        assert_eq!(classify("好的", MODERATE), None);
    }

    #[test]
    fn reason_identifiers_are_stable() {
        // The frontend maps these to viewer-facing text; renaming one silently
        // downgrades the overlay to its generic fallback hint.
        assert_eq!(OcrRejection::TooShort.as_str(), "tooShort");
        assert_eq!(OcrRejection::Untranslatable.as_str(), "untranslatable");
    }
}
