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
}
