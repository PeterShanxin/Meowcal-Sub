use super::*;
use std::time::Duration;

#[test]
fn test_new_context() {
    let ctx = TranslationContext::new(500, true);
    assert_eq!(ctx.budget_tokens, 500);
    assert!(ctx.enabled);
    assert!(ctx.memory.is_none());
    assert!(ctx.history.is_empty());
}

#[test]
fn test_duplicate_detection() {
    let mut ctx = TranslationContext::new(500, true);

    // First text is not a duplicate
    assert!(!ctx.is_duplicate("Hello world"));

    // Add it to history
    ctx.add_ocr_line(
        "Hello world",
        Instant::now(),
        12,
        300,
        Duration::from_secs(10),
    );

    // Same text should be duplicate
    assert!(ctx.is_duplicate("Hello world"));

    // Similar text should be duplicate
    assert!(ctx.is_duplicate("hello world"));

    // Different text should not be duplicate
    assert!(!ctx.is_duplicate("Goodbye world"));
}

#[test]
fn test_context_prompt() {
    let mut ctx = TranslationContext::new(500, true);

    // No context initially
    assert!(ctx.build_context_prompt().is_none());

    // Add some history
    let now = Instant::now();
    ctx.add_ocr_line("Hello", now, 12, 300, Duration::from_secs(10));
    ctx.add_ocr_line("World", now, 12, 300, Duration::from_secs(10));

    let prompt = ctx.build_context_prompt();
    assert!(prompt.is_some());
    let prompt = prompt.unwrap();
    assert!(prompt.contains("Hello"));
    assert!(prompt.contains("World"));
}

#[test]
fn test_disabled_context() {
    let mut ctx = TranslationContext::new(500, false);

    // When disabled, nothing should accumulate
    assert!(!ctx.is_duplicate("Hello"));
    ctx.add_ocr_line("Hello", Instant::now(), 12, 300, Duration::from_secs(10));
    assert!(ctx.history.is_empty());
    assert!(ctx.build_context_prompt().is_none());
}

#[test]
fn test_memory_prompt_excludes_recent() {
    let mut ctx = TranslationContext::new(500, true);
    ctx.set_memory("Genre: drama. Names: X->Y".to_string());
    ctx.add_ocr_line("Hello", Instant::now(), 12, 300, Duration::from_secs(10));

    let prompt = ctx.build_memory_prompt(600).unwrap();
    assert!(prompt.contains("Genre:"));
    assert!(!prompt.contains("Hello"));
}

#[test]
fn test_memory_truncation_hard_cap() {
    let mut ctx = TranslationContext::new(200, true);
    let long = "a".repeat(2000);
    ctx.set_memory(long);

    let mem = ctx.memory().unwrap_or_default();
    let budget = ctx.memory_token_budget();
    assert!(TranslationContext::estimate_tokens(mem) <= budget);
}

#[test]
fn test_token_estimation() {
    // ASCII text
    let ascii_tokens = TranslationContext::estimate_tokens("Hello world");
    assert!(ascii_tokens > 0 && ascii_tokens < 10);

    // CJK text (each char ~1 token)
    let cjk_tokens = TranslationContext::estimate_tokens("你好世界");
    assert_eq!(cjk_tokens, 4);
}

#[test]
fn test_compression_threshold() {
    // Budget is clamped to MIN_TOKEN_BUDGET (200), so ensure we exceed the threshold.
    let mut ctx = TranslationContext::new(200, true);

    // Add entries until we hit threshold
    for i in 0..40 {
        ctx.add_ocr_line(
            &format!("Source text number {}", i),
            Instant::now(),
            100,
            300,
            Duration::from_secs(10),
        );
    }

    // Should need compression now
    assert!(ctx.needs_compression());
}
