// =============================================================================
// OCR GATE - why a recognised line never reaches the translator
// =============================================================================
// OCR returning text is not the same as OCR returning *usable* text. The capture
// loop drops lines that are too unreliable to translate, and until this module
// existed it dropped them silently: the overlay kept whatever notice was already
// on screen, which made an unreadable region indistinguishable from an empty one.
//
// Keeping the decision here (instead of inline in the capture loop) means the
// reason is a value the pipeline can report to the viewer, and that every
// threshold is unit-testable without a screen, a webview, or Windows OCR.
// =============================================================================

/// Why a recognised OCR line was not worth translating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrRejection {
    /// Windows OCR recognised glyphs but is not confident they are correct.
    LowConfidence,
    /// Fewer than two alphanumeric characters - specks, borders, or subtitle
    /// edges caught mid-fade rather than a line of dialogue.
    TooShort,
    /// Real characters, but nothing a translator can act on (digits, symbols,
    /// timestamps).
    Untranslatable,
}

impl OcrRejection {
    /// Stable identifier shared with the frontend, which owns the wording.
    pub fn as_str(self) -> &'static str {
        match self {
            OcrRejection::LowConfidence => "lowConfidence",
            OcrRejection::TooShort => "tooShort",
            OcrRejection::Untranslatable => "untranslatable",
        }
    }
}

/// Minimum alphanumeric characters before a line counts as dialogue.
const MIN_SIGNIFICANT_CHARS: usize = 2;

/// Decide whether a recognised line should be translated.
///
/// `None` means the line is good enough to send onward. The checks run cheapest
/// first so a noisy region does not pay for the untranslatable-text scan on
/// every frame.
pub fn classify(text: &str, confidence: f32, confidence_threshold: f32) -> Option<OcrRejection> {
    if confidence < confidence_threshold {
        return Some(OcrRejection::LowConfidence);
    }

    let significant_chars = text.chars().filter(|ch| ch.is_alphanumeric()).count();
    if significant_chars < MIN_SIGNIFICANT_CHARS {
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

    #[test]
    fn accepts_a_confident_line_of_dialogue() {
        assert_eq!(classify("Where are you going?", 0.92, 0.6), None);
    }

    #[test]
    fn rejects_confidence_below_the_threshold() {
        assert_eq!(
            classify("Where are you going?", 0.4, 0.6),
            Some(OcrRejection::LowConfidence)
        );
    }

    #[test]
    fn confidence_is_checked_before_content() {
        // A blurry frame must report the blur, not the shape of what it guessed.
        assert_eq!(classify("-", 0.1, 0.6), Some(OcrRejection::LowConfidence));
    }

    #[test]
    fn rejects_a_single_alphanumeric_character() {
        assert_eq!(classify("a.", 0.99, 0.6), Some(OcrRejection::TooShort));
    }

    #[test]
    fn rejects_punctuation_only_noise() {
        assert_eq!(classify("--- ...", 0.99, 0.6), Some(OcrRejection::TooShort));
    }

    #[test]
    fn accepts_exactly_two_alphanumeric_characters() {
        assert_eq!(classify("Hi", 0.99, 0.6), None);
    }

    #[test]
    fn reason_identifiers_are_stable() {
        // The frontend maps these to viewer-facing text; renaming one silently
        // downgrades the overlay to its generic fallback hint.
        assert_eq!(OcrRejection::LowConfidence.as_str(), "lowConfidence");
        assert_eq!(OcrRejection::TooShort.as_str(), "tooShort");
        assert_eq!(OcrRejection::Untranslatable.as_str(), "untranslatable");
    }
}
