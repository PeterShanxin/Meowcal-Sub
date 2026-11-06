use meowcal_core::ocr::Engine;

/// Convert Windows OCR language aliases to the stable tags used by app config.
///
/// Windows may report script-qualified tags such as `zh-Hans-CN`, while older
/// Meowcal Sub configs and the UI use `zh-CN`. Keep the normalization at the
/// WinRT boundary so every caller benefits, including migrated configs.
pub fn normalize_language_tag(tag: &str) -> String {
    let normalized = tag.trim().replace('_', "-");
    let lower = normalized.to_ascii_lowercase();
    let parts: Vec<&str> = lower.split('-').collect();

    if parts.first() == Some(&"zh") {
        let has_hans = parts.contains(&"hans");
        let has_hant = parts.contains(&"hant");
        let region = parts.last().copied();
        if has_hans || region == Some("cn") || region == Some("sg") {
            return "zh-CN".to_string();
        }
        if has_hant || matches!(region, Some("tw" | "hk" | "mo")) {
            return "zh-TW".to_string();
        }
    }

    normalized
}

/// The recognizer a session reading `source_language` uses. PP-OCRv6's
/// recognizer has no Hangul, so Korean reads with Windows OCR whatever the
/// setting says. `src/ui/recognition.ts` applies the same rule to readiness.
pub fn session_engine(configured: Engine, source_language: &str) -> Engine {
    if source_language
        .trim()
        .to_ascii_lowercase()
        .starts_with("ko")
    {
        Engine::Windows
    } else {
        configured
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_language_tag, session_engine, Engine};

    #[test]
    fn korean_sessions_read_with_windows_ocr_whatever_the_setting() {
        assert_eq!(session_engine(Engine::PpOcrV6, "ko-KR"), Engine::Windows);
        assert_eq!(session_engine(Engine::PpOcrV6, "zh-CN"), Engine::PpOcrV6);
        assert_eq!(session_engine(Engine::PpOcrV6, "ja-JP"), Engine::PpOcrV6);
        assert_eq!(session_engine(Engine::Windows, "en-US"), Engine::Windows);
    }

    #[test]
    fn normalizes_simplified_chinese_aliases() {
        for alias in ["zh-Hans-CN", "zh-Hans", "zh_CN", "zh-SG"] {
            assert_eq!(normalize_language_tag(alias), "zh-CN");
        }
    }

    #[test]
    fn normalizes_traditional_chinese_aliases() {
        for alias in ["zh-Hant-TW", "zh-Hant", "zh_HK", "zh-MO"] {
            assert_eq!(normalize_language_tag(alias), "zh-TW");
        }
    }

    #[test]
    fn leaves_other_tags_unchanged_apart_from_separator_cleanup() {
        assert_eq!(normalize_language_tag(" ja-JP "), "ja-JP");
        assert_eq!(normalize_language_tag("en_US"), "en-US");
    }
}
