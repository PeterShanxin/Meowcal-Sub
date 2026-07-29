use super::*;

#[test]
fn test_default_config() {
    let config = AppConfig::default();
    assert_eq!(config.source_language, "en-US");
    assert_eq!(config.target_language, "zh-CN");
    assert_eq!(config.capture_interval_ms, 500);
}

#[test]
fn test_capture_region_valid() {
    let region = CaptureRegion::new(0, 0, 100, 50);
    assert!(region.is_valid());
    assert_eq!(region.area(), 5000);
}

#[test]
fn test_capture_region_invalid() {
    let region = CaptureRegion::new(0, 0, 0, 50);
    assert!(!region.is_valid());
}

#[test]
fn test_translation_config_defaults() {
    let config = TranslationConfig::default();
    assert!(config.enable_foundry_local);
    assert!(config.allow_mock_fallback);
    assert!(config.enable_context_aware);
    assert_eq!(config.context_level, ContextLevel::MemoryAndRecent);
    assert_eq!(config.context_recent_count, 3);
    assert_eq!(config.context_budget_percent, 15);
    assert_eq!(config.context_summary_cooldown_ms, 5_000);
    assert_eq!(config.prompt_max_source_chars, 300);
    assert_eq!(config.prompt_max_context_chars, 600);
    assert_eq!(config.context_buffer_size, 12);
    assert_eq!(config.context_reset_gap_ms, 6_000);
}

#[test]
fn test_translation_config_serialization() {
    let config = TranslationConfig::default();
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("enableContextAware"));
    assert!(json.contains("contextLevel"));

    let deserialized: TranslationConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(
        deserialized.enable_context_aware,
        config.enable_context_aware
    );
    assert_eq!(deserialized.context_level, config.context_level);
}

#[test]
fn test_translation_config_missing_field_uses_default() {
    let json = r#"{
        "enableFoundryLocal": true,
        "allowMockFallback": true,
        "foundryLocal": {}
    }"#;
    let config: TranslationConfig = serde_json::from_str(json).unwrap();
    assert!(config.enable_context_aware);
}
