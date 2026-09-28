use super::TranslationContext;
use std::time::{Duration, Instant};

#[test]
fn context_scene_expiry_survives_complete_budget_eviction() {
    let mut context = TranslationContext::new(200, true);
    let now = Instant::now();
    let gap = Duration::from_secs(6);
    let source = "旧场景".repeat(100);
    context.add_ocr_line(&source, now, 12, 2_000, gap);
    context.set_memory("Previous scene characters and setting.".into());
    assert_eq!(context.history_len(), 0, "compression evicted all entries");
    assert!(context.build_context_prompt().is_some());
    assert!(!context.reset_if_stale(now + Duration::from_secs(5), gap));
    assert!(context.reset_if_stale(now + Duration::from_secs(7), gap));
    assert!(context.build_context_prompt().is_none());
    assert!(!context.is_duplicate(&source));
    assert!(!context.reset_if_stale(now + Duration::from_secs(20), gap));
}

#[test]
fn context_scene_duplicates_refresh_activity_but_noise_does_not() {
    let mut context = TranslationContext::new(500, true);
    let now = Instant::now();
    let gap = Duration::from_secs(6);
    context.add_ocr_line("Wait by the door.", now, 12, 300, gap);
    context.add_ocr_line(
        "Wait by the door.",
        now + Duration::from_secs(5),
        12,
        300,
        gap,
    );
    context.add_ocr_line("...", now + Duration::from_secs(8), 12, 300, gap);
    assert!(!context.reset_if_stale(now + Duration::from_secs(10), gap));
    assert!(context.reset_if_stale(now + Duration::from_secs(12), gap));
}

#[test]
fn context_scene_restoration_does_not_rewind_activity() {
    let mut context = TranslationContext::new(500, true);
    let now = Instant::now();
    let gap = Duration::from_secs(6);
    context.add_ocr_line("Current scene.", now, 12, 300, gap);
    context.restore_history_entries(vec![super::HistoryEntry {
        text: "Older scene.".into(),
        timestamp: now - Duration::from_secs(60),
        token_estimate: 3,
    }]);
    assert!(!context.reset_if_stale(now + Duration::from_secs(5), gap));
    assert!(context.reset_if_stale(now + Duration::from_secs(7), gap));
}
