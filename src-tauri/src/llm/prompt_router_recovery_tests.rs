use super::*;

#[test]
fn language_recovery_changes_the_instruction_and_omits_context() {
    for (source, target, label) in [
        ("下一班列车将在五分钟后到站", "en-US", "English"),
        ("The train arrives soon.", "zh-CN", "Chinese"),
    ] {
        let options = PromptRouterOptions {
            enable_context: true,
            retry_wrong_language: true,
            max_context_chars: 600,
            max_source_chars: 300,
        };
        let recovery = build_subtitle_translation_prompt(
            source,
            None,
            target,
            Some("unrelated context"),
            options,
        )
        .unwrap();
        let initial = build_subtitle_translation_prompt(
            source,
            None,
            target,
            None,
            PromptRouterOptions {
                retry_wrong_language: false,
                ..options
            },
        )
        .unwrap();
        assert!(!recovery.used_context);
        assert!(!recovery.prompt.contains("unrelated context"));
        assert!(recovery
            .prompt
            .contains(&format!("Output only the {label} translation")));
        assert!(recovery.prompt.ends_with(source));
        assert_ne!(recovery.prompt, initial.prompt);
    }
}
