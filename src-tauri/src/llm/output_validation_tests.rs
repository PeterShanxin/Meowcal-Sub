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
        (
            "需要鳗鱼。",
            "需要鲨鱼。",
            TranslationOutputRejection::WrongLanguage,
        ),
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

// French, Spanish and German are supported sources, and their echoes carry
// accented letters. Counting only ASCII letters put those below the Latin
// share the guard needs, so a source echo the guard exists to catch was
// accepted and displayed as a Chinese translation.
#[test]
fn rejects_accented_latin_output_when_the_target_is_chinese() {
    for (source, translated) in [
        ("Ça va déjà?", "Ça va déjà?"),
        ("¿Dónde está la estación?", "¿Dónde está la estación?"),
        ("Grüße für die Prüfung", "Grüße für die Prüfung"),
    ] {
        assert_eq!(
            validate_translation_output(source, translated, "fr-FR", "zh-CN"),
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

// The same benefit of the doubt has to survive counting accented letters as
// Latin. `Saber` clears the length floor and is accepted on length alone;
// `Dvořák`, `François` and `Pokémon` are the same kind of output one character
// longer, and counting their accents as Latin took their share from below the
// threshold to 6/6 and turned a name into a quality notice.
#[test]
fn allows_an_accented_proper_name_for_a_cjk_target() {
    for name in [
        "Dvořák",
        "François",
        "Pokémon",
        "Beyoncé",
        "Müller",
        "Zoë",
        "Dvořák.",
    ] {
        assert!(
            validate_translation_output(name, name, "en-US", "zh-CN").is_ok(),
            "{name:?} is a name, not a wrong-language result"
        );
    }
}

// ...and the exemption has to stay narrow enough that the reads it was built to
// catch still fail. Every one of these is a single token too, so the shape of
// the token is what separates them from a name.
#[test]
fn the_proper_name_exemption_does_not_admit_garbled_single_tokens() {
    for text in [
        // Capitals in the middle: OCR noise, not a name.
        "MFtiÄhave",
        // Digits and an underscore inside a word.
        "R_4gng",
        // No initial capital, so not a name however it is spelled.
        "thinnedput",
        "déjàdéjàdéjà",
    ] {
        assert_eq!(
            validate_translation_output(text, text, "en-US", "zh-CN"),
            Err(TranslationOutputRejection::WrongLanguage),
            "{text:?} should still be rejected for a Chinese target"
        );
    }
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
