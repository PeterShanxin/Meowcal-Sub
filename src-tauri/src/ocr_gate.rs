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
    /// More text than a subtitle cue can hold, so the region is catching page
    /// furniture - a menu, a stat readout, a wall of body text - rather than a
    /// line of dialogue.
    TooLong,
    /// Recognised characters, but most of the line is OCR noise - symbols wedged
    /// inside words, as in `Wh€reythe MFtiÄhave relatively;thinnedput?`. The
    /// model can only render nonsense from it, and spending the one translation
    /// slot on it costs the real dialogue behind it.
    Garbled,
    /// Text was read, but every line of it sat in a band judged not to carry
    /// dialogue. Distinct from an empty region because the two need opposite
    /// responses, and reporting this as "no text detected" is what left the
    /// blackout in issue #59 undiagnosable from a user's log.
    BandHeld,
}

impl OcrRejection {
    /// Stable identifier shared with the frontend, which owns the wording.
    pub fn as_str(self) -> &'static str {
        match self {
            OcrRejection::TooShort => "tooShort",
            OcrRejection::Untranslatable => "untranslatable",
            OcrRejection::TooLong => "tooLong",
            OcrRejection::Garbled => "garbled",
            OcrRejection::BandHeld => "bandHeld",
        }
    }
}

// The longest line still worth translating.
//
// This is a subtitle app: a cue is one or two short lines, and anything much
// longer is the capture region taking in the page around the subtitle. Sending
// it on is worse than dropping it - it spends a full model call, holds the one
// translation slot for seconds while real dialogue goes by, and puts a
// paragraph of unrelated text over the video.
//
// Two limits, because the same sentence needs roughly twice the characters in
// an alphabetic script as in an ideographic one. A single cap would either wave
// a page of Chinese through or cut an ordinary English cue in half. Broadcast
// practice puts a full two-line cue near 40 characters of CJK and near 84 of
// Latin, so both limits sit above anything a real cue produces.
const MAX_CJK_CHARS: usize = 50;
const MAX_ALPHABETIC_CHARS: usize = 100;

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

    if text.chars().count() > length_limit(text) {
        return Some(OcrRejection::TooLong);
    }

    if crate::llm::is_untranslatable_text(text) {
        return Some(OcrRejection::Untranslatable);
    }

    // Last, because it is the most expensive test and the least certain. A line
    // that failed any rule above was never going to be translated anyway.
    if crate::ocr_corruption::is_mostly_noise(text) {
        return Some(OcrRejection::Garbled);
    }

    None
}

/// Pick the length limit that matches the script the line is written in.
///
/// Decided per line rather than from the configured source language: the
/// language setting says what the viewer expects, and the limit has to hold for
/// whatever OCR actually returned.
fn length_limit(text: &str) -> usize {
    let cjk = text
        .chars()
        .filter(|ch| crate::llm::text_utils::is_cjk_char(*ch))
        .count();
    let alphabetic = text
        .chars()
        .filter(|ch| ch.is_alphabetic() && !crate::llm::text_utils::is_cjk_char(*ch))
        .count();

    if cjk > alphabetic {
        MAX_CJK_CHARS
    } else {
        MAX_ALPHABETIC_CHARS
    }
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
        assert_eq!(OcrRejection::TooLong.as_str(), "tooLong");
        assert_eq!(OcrRejection::Garbled.as_str(), "garbled");
        assert_eq!(OcrRejection::BandHeld.as_str(), "bandHeld");
    }

    // Verbatim from the session in issue #59. Each of these reached the model and
    // could only come back as nonsense, and each held the one translation slot
    // while real dialogue went past.
    #[test]
    fn rejects_a_line_that_is_mostly_ocr_noise() {
        assert_eq!(
            classify("Wh€reythe MFtiÄhave relatively;thinnedput?", MODERATE),
            Some(OcrRejection::Garbled)
        );
    }

    // Two tokens is not enough of a sample to condemn a line, however bad one of
    // them looks - see `ocr_corruption::MIN_TOKENS_TO_JUDGE`. Dropping a real
    // subtitle is permanent; letting a mangled one through costs 250ms.
    #[test]
    fn a_short_line_is_not_rejected_on_a_single_bad_token() {
        assert_eq!(classify("of qrßinary•Magebraft.", MODERATE), None);
    }

    // Chinese has no spaces, so the whole line is one token and one ordinary
    // character used to condemn all of it.
    #[test]
    fn an_ordinary_chinese_line_is_not_rejected_as_noise() {
        assert_eq!(classify("他说:我们明天再谈吧", MODERATE), None);
        assert_eq!(classify("第一季/第二季都很精彩", MODERATE), None);
    }

    // The other side of the same rule. Dropping a real subtitle is the worse
    // error, so a line carrying some noise but mostly text still goes through.
    #[test]
    fn accepts_a_line_that_is_mostly_readable() {
        assert_eq!(
            classify("bf//dzz:: However, isn't he a hero from an era", MODERATE),
            None
        );
        assert_eq!(
            classify("where the Mystics have relatively thinned out?", MODERATE),
            None
        );
    }

    #[test]
    fn accepts_a_full_two_line_chinese_cue() {
        let cue = "如果想完全复刻再展开的话就会变得非常困难所以先做一个简单的版本";
        assert!(cue.chars().count() <= MAX_CJK_CHARS);
        assert_eq!(classify(cue, MODERATE), None);
    }

    #[test]
    fn rejects_a_page_of_chinese_text() {
        let page = "好".repeat(MAX_CJK_CHARS + 1);
        assert_eq!(classify(&page, MODERATE), Some(OcrRejection::TooLong));
        // And the character before it still passes, so the limit is where the
        // constant says it is rather than somewhere near it.
        assert_eq!(classify(&"好".repeat(MAX_CJK_CHARS), MODERATE), None);
    }

    // The reason there are two limits. Judged against the CJK cap this ordinary
    // two-line cue would be thrown away for being ordinary.
    #[test]
    fn accepts_a_long_two_line_english_cue() {
        let cue =
            "I never expected to see you here again after everything that happened last winter";
        assert!(cue.chars().count() > MAX_CJK_CHARS);
        assert!(cue.chars().count() <= MAX_ALPHABETIC_CHARS);
        assert_eq!(classify(cue, MODERATE), None);
    }

    // The failure this gate exists for: a capture region covering most of the
    // screen hands back every line on the page joined into one sentence, and
    // the translator spends a full model call rendering it as nonsense.
    #[test]
    fn rejects_the_page_furniture_a_tall_region_catches() {
        let page = "Name Status CPU Memory Disk Network Processes 91.0 MB 82.3 MB 45.1 MB \
                    System 12.4 MB Background processes 34 running";
        assert!(page.chars().count() > MAX_ALPHABETIC_CHARS);
        assert_eq!(classify(page, MODERATE), Some(OcrRejection::TooLong));
    }

    #[test]
    fn a_chinese_line_carrying_a_latin_name_is_still_judged_as_chinese() {
        let mixed = format!("{}Saber", "好".repeat(MAX_CJK_CHARS));
        assert!(mixed.chars().count() <= MAX_ALPHABETIC_CHARS);
        assert_eq!(classify(&mixed, MODERATE), Some(OcrRejection::TooLong));
    }
}
