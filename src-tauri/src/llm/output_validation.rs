use super::text_utils::is_cjk_char;
use std::collections::HashSet;

// Absolute runaway-generation guard. Counts characters rather than UTF-8 bytes.
const MAX_SUBTITLE_OUTPUT_CHARS: usize = 240;
const DEFAULT_OUTPUT_RATIO: usize = 4;
const MIN_SHORT_OUTPUT_CHARS: usize = 32;
// Short CJK phrases routinely expand into much longer natural English.
const CJK_TO_ENGLISH_RATIO: usize = 12;
const MIN_CJK_TO_ENGLISH_CHARS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TranslationOutputRejection {
    EmptyOutput,
    TooLong,
    RepetitionLoop,
    PromptEcho,
    WrongLanguage,
}

impl TranslationOutputRejection {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::EmptyOutput => "empty_output",
            Self::TooLong => "too_long",
            Self::RepetitionLoop => "repetition_loop",
            Self::PromptEcho => "prompt_echo",
            Self::WrongLanguage => "wrong_language",
        }
    }
}

pub(crate) fn validate_translation_output(
    source_text: &str,
    translated: &str,
    source_language: &str,
    target_language: &str,
) -> Result<(), TranslationOutputRejection> {
    let source_chars = source_text.chars().count().max(1);
    let translated_chars = translated.chars().count();
    if translated_chars == 0 {
        return Err(TranslationOutputRejection::EmptyOutput);
    }
    if looks_like_prompt_echo(translated) {
        return Err(TranslationOutputRejection::PromptEcho);
    }

    let cjk_to_english =
        is_cjk_source(source_language, source_text) && is_english_target(target_language);
    let (ratio, minimum) = if cjk_to_english {
        (CJK_TO_ENGLISH_RATIO, MIN_CJK_TO_ENGLISH_CHARS)
    } else {
        (DEFAULT_OUTPUT_RATIO, MIN_SHORT_OUTPUT_CHARS)
    };
    if translated_chars > MAX_SUBTITLE_OUTPUT_CHARS
        || translated_chars > source_chars.saturating_mul(ratio).max(minimum)
    {
        return Err(TranslationOutputRejection::TooLong);
    }
    if looks_repetition_loop(translated) {
        return Err(TranslationOutputRejection::RepetitionLoop);
    }
    if is_english_target(target_language) && is_probably_non_english_for_en_target(translated) {
        return Err(TranslationOutputRejection::WrongLanguage);
    }
    if is_cjk_target(target_language) && is_probably_not_cjk_for_cjk_target(translated) {
        return Err(TranslationOutputRejection::WrongLanguage);
    }
    Ok(())
}

pub(crate) fn quality_issue_message(reason: TranslationOutputRejection) -> String {
    match reason {
        TranslationOutputRejection::TooLong => {
            "Translation output rejected as corrupted (overlong output)."
        }
        TranslationOutputRejection::RepetitionLoop => {
            "Translation output rejected as corrupted (repetitive output)."
        }
        TranslationOutputRejection::PromptEcho => {
            "Translation output rejected as corrupted (prompt echo)."
        }
        TranslationOutputRejection::WrongLanguage => {
            "Translation output rejected as corrupted (incorrect output language)."
        }
        TranslationOutputRejection::EmptyOutput => {
            "Translation output rejected as corrupted (empty output)."
        }
    }
    .to_string()
}

fn is_cjk_source(source_language: &str, source_text: &str) -> bool {
    let language = source_language
        .split('-')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(language.as_str(), "zh" | "ja" | "ko") || source_text.chars().any(is_cjk_char)
}

fn looks_like_prompt_echo(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("you are a subtitle translator")
        || (lower.contains("translate the subtitle") && lower.contains("subtitle:"))
        || (lower.contains("translate the following subtitle") && lower.contains("subtitle:"))
}

fn looks_repetition_loop(text: &str) -> bool {
    let tokens = tokenize_for_repetition(text);
    if tokens.len() < 8 {
        return false;
    }
    let mut streak = 1usize;
    let mut max_streak = 1usize;
    for i in 1..tokens.len() {
        if tokens[i] == tokens[i - 1] {
            streak += 1;
            max_streak = max_streak.max(streak);
        } else {
            streak = 1;
        }
    }
    if max_streak >= 4 {
        return true;
    }

    let unique: HashSet<&str> = tokens.iter().map(String::as_str).collect();
    if unique.len().saturating_mul(3) <= tokens.len() {
        return true;
    }
    if text.matches('/').count() >= 5 {
        let parts: Vec<String> = text
            .split('/')
            .map(|part| part.trim().to_ascii_lowercase())
            .filter(|part| !part.is_empty())
            .collect();
        if parts.len() >= 5 {
            let unique_parts: HashSet<&str> = parts.iter().map(String::as_str).collect();
            if unique_parts.len().saturating_mul(2) <= parts.len() {
                return true;
            }
        }
    }
    false
}

fn tokenize_for_repetition(text: &str) -> Vec<String> {
    text.split(|ch: char| !(ch.is_alphanumeric() || is_cjk_char(ch)))
        .map(|token| token.trim().to_ascii_lowercase())
        .filter(|token| !token.is_empty())
        .collect()
}

fn is_english_target(target_language: &str) -> bool {
    target_language
        .split('-')
        .next()
        .map(|lang| lang.eq_ignore_ascii_case("en"))
        .unwrap_or(false)
}

/// Shortest output worth judging by script.
///
/// A one-word cue can legitimately come back as a proper noun the model chose
/// not to render - and a name is exactly the case where Latin-only output is
/// correct rather than broken. Below this the benefit of the doubt is cheaper
/// than the alternative: rejecting turns a usable line into a quality notice.
const MIN_CHARS_TO_JUDGE_SCRIPT: usize = 6;

fn is_cjk_target(target_language: &str) -> bool {
    let language = target_language
        .split('-')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(language.as_str(), "zh" | "ja" | "ko")
}

/// Whether output for a CJK target came back in the wrong script entirely.
///
/// The mirror of `is_probably_non_english_for_en_target`, which only ever ran
/// when the target was English. With `zh-CN` selected there was no equivalent
/// check, so Latin noise passed validation and was displayed as a translation:
/// `MFtiÄhave` came back as `MFtiÄhave`, `R_4gng` as `R_4gng`. See issue #59.
///
/// Any CJK at all is enough to pass. A translation that keeps a name in Latin
/// script - `Imageong • 设置 — 更新功能让一切变得混乱` - is a real translation, and
/// judging it by proportion would reject exactly the mixed lines that are right.
fn is_probably_not_cjk_for_cjk_target(text: &str) -> bool {
    let mut alphabetic_total = 0usize;
    let mut latin_letters = 0usize;
    let mut non_whitespace_chars = 0usize;
    for ch in text.chars() {
        if !ch.is_whitespace() {
            non_whitespace_chars += 1;
        }
        if is_cjk_char(ch) {
            return false;
        }
        if ch.is_alphabetic() {
            alphabetic_total += 1;
            if ch.is_ascii_alphabetic() {
                latin_letters += 1;
            }
        }
    }
    // No letters at all is a timestamp, a number, a row of punctuation - none of
    // which a CJK translation would have added characters to.
    if latin_letters == 0 || non_whitespace_chars < MIN_CHARS_TO_JUDGE_SCRIPT {
        return false;
    }
    latin_letters.saturating_mul(10) >= alphabetic_total.saturating_mul(7)
}

fn is_probably_non_english_for_en_target(text: &str) -> bool {
    let mut alphabetic_total = 0usize;
    let mut latin_letters = 0usize;
    let mut cjk_letters = 0usize;
    let mut non_whitespace_chars = 0usize;
    for ch in text.chars() {
        if !ch.is_whitespace() {
            non_whitespace_chars += 1;
        }
        if ch.is_alphabetic() {
            alphabetic_total += 1;
            if ch.is_ascii_alphabetic() {
                latin_letters += 1;
            }
        }
        if is_cjk_char(ch) {
            cjk_letters += 1;
        }
    }
    if cjk_letters == 0 {
        return false;
    }
    if latin_letters == 0 {
        return true;
    }
    if non_whitespace_chars < 6 && cjk_letters <= 1 {
        return false;
    }
    if non_whitespace_chars < 10 && cjk_letters < 3 {
        return false;
    }
    if cjk_letters.saturating_mul(100) < non_whitespace_chars.saturating_mul(30) {
        return false;
    }
    if alphabetic_total == 0 {
        return true;
    }
    latin_letters.saturating_mul(10) < alphabetic_total.saturating_mul(7)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zh_cn_to_en_extreme_output_as_too_long() {
        let translated = "a".repeat(150);
        assert_eq!(
            validate_translation_output("你好", &translated, "zh-CN", "en-US"),
            Err(TranslationOutputRejection::TooLong)
        );
    }

    #[test]
    fn allows_realistic_short_cjk_to_english_expansion() {
        for (source, translated, language) in [
            ("谢谢", "Thank you.", "zh-CN"),
            (
                "先不提时钟塔",
                "Let's not talk about the clock tower for now.",
                "zh-CN",
            ),
            ("もういい", "That's enough for now.", "ja-JP"),
        ] {
            assert!(
                validate_translation_output(source, translated, language, "en-US").is_ok(),
                "{language} case should pass"
            );
        }
    }

    #[test]
    fn returns_stable_rejection_reasons() {
        let cases = [
            ("你好", "", TranslationOutputRejection::EmptyOutput),
            (
                "这是一个足够长的字幕文本用于测试",
                "go go go go go go go go",
                TranslationOutputRejection::RepetitionLoop,
            ),
            (
                "你好",
                "You are a subtitle translator. Translate the subtitle into English. Subtitle: 你好",
                TranslationOutputRejection::PromptEcho,
            ),
            ("需要鳗鱼。", "需要鲨鱼。", TranslationOutputRejection::WrongLanguage),
        ];
        for (source, translated, expected) in cases {
            assert_eq!(
                validate_translation_output(source, translated, "zh-CN", "en-US"),
                Err(expected)
            );
        }
    }

    #[test]
    fn allows_mixed_and_non_english_target_cases() {
        assert!(validate_translation_output("需要", "OK 好", "zh-CN", "en-US").is_ok());
        assert!(validate_translation_output("Need eel.", "需要鲨鱼。", "en-US", "zh-CN",).is_ok());
    }

    // The mirror gap in issue #59. With `zh-CN` selected there was no
    // wrong-language check at all, so Latin noise passed validation and was
    // shown as a translation. The first two are quoted from that session; the
    // third is the same shape.
    #[test]
    fn rejects_latin_output_when_the_target_is_chinese() {
        for (source, translated) in [
            ("MFtiÄhave", "MFtiÄhave"),
            ("R_4gng", "R_4gng"),
            ("thinnedput", "thinnedput"),
        ] {
            assert_eq!(
                validate_translation_output(source, translated, "en-US", "zh-CN"),
                Err(TranslationOutputRejection::WrongLanguage),
                "{translated:?} should be rejected for a Chinese target"
            );
        }
    }

    // A translation that keeps a name in Latin script is a real translation, and
    // the longest-lived garbage line of that session was rejected while this
    // shape has to survive.
    #[test]
    fn allows_a_chinese_translation_that_carries_latin_text() {
        assert!(validate_translation_output(
            "Imageong - Settings - Updates Discombobulating",
            "Imageong • 设置 — 更新功能让一切变得混乱",
            "en-US",
            "zh-CN",
        )
        .is_ok());
    }

    // Short output gets the benefit of the doubt: a one-word cue can legitimately
    // come back as a name, and rejecting turns a usable line into a notice.
    #[test]
    fn allows_short_latin_output_and_output_with_no_letters() {
        assert!(validate_translation_output("Saber", "Saber", "en-US", "zh-CN").is_ok());
        assert!(validate_translation_output("12:30", "12:30", "en-US", "zh-CN").is_ok());
    }

    // Japanese and Korean targets get the same guard; only the language tag
    // differs, and reading it wrongly would leave those targets unprotected.
    #[test]
    fn the_guard_covers_the_other_cjk_targets() {
        for target in ["ja-JP", "ko-KR", "zh-Hans-CN"] {
            assert_eq!(
                validate_translation_output("MFtiÄhave", "MFtiÄhave", "en-US", target),
                Err(TranslationOutputRejection::WrongLanguage),
                "{target} should be treated as a CJK target"
            );
        }
    }
}
