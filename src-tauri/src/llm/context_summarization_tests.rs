use super::fixtures::*;
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::watch;

#[tokio::test(start_paused = true)]
async fn nothing_is_scheduled_when_compression_is_not_needed() {
    let manager = manager(test_config());
    let summarizer = Arc::new(FakeSummarizer::with_responses(vec![ok("unused")]));
    let h = harness(manager, summarizer.clone(), 0);

    h.scheduler.schedule_if_needed(0, h.rx.clone());
    advance(1_000).await;
    settle().await;

    assert_eq!(summarizer.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn the_cooldown_suppresses_a_reschedule_and_expires() {
    let manager = manager(test_config());
    let summarizer = Arc::new(FakeSummarizer::with_responses(vec![
        ok("Genre: drama. Names: A"),
        ok("Genre: comedy"),
    ]));
    let h = harness(manager.clone(), summarizer.clone(), 10_000);

    fill(&manager, 5, FILLER);
    assert!(manager.needs_context_compression());

    h.scheduler.schedule_if_needed(10_000, h.rx.clone());
    advance(901).await;
    settle().await;
    assert_eq!(summarizer.calls(), 1);
    let prompt = manager.get_context_prompt().expect("memory exists");
    assert!(prompt.contains("Genre: drama"));

    // History drains and memory updates; more lines push it over the
    // threshold again, but the cooldown has not expired.
    fill(&manager, 2, FILLER);
    assert!(manager.needs_context_compression());

    h.scheduler.schedule_if_needed(15_000, h.rx.clone());
    advance(901).await;
    settle().await;
    assert_eq!(summarizer.calls(), 1, "cooldown should suppress the run");

    h.scheduler.schedule_if_needed(20_001, h.rx.clone());
    advance(901).await;
    settle().await;
    assert_eq!(summarizer.calls(), 2, "expired cooldown allows the run");
}

#[tokio::test(start_paused = true)]
async fn a_zero_cooldown_bypasses_the_gate() {
    let manager = manager(test_config());
    let summarizer = Arc::new(FakeSummarizer::with_responses(vec![
        ok("Genre: drama"),
        ok("Genre: comedy"),
    ]));
    let h = harness(manager.clone(), summarizer.clone(), 0);

    fill(&manager, 5, FILLER);
    h.scheduler.schedule_if_needed(5_000, h.rx.clone());
    advance(901).await;
    settle().await;
    assert_eq!(summarizer.calls(), 1);

    fill(&manager, 2, FILLER);
    assert!(manager.needs_context_compression());
    h.scheduler.schedule_if_needed(5_000, h.rx.clone());
    advance(901).await;
    settle().await;
    assert_eq!(summarizer.calls(), 2);
}

#[tokio::test(start_paused = true)]
async fn an_in_flight_run_suppresses_a_duplicate_schedule() {
    let manager = manager(test_config());
    let summarizer = Arc::new(FakeSummarizer::with_responses(vec![ok("Genre: drama")]));
    let h = harness(manager.clone(), summarizer.clone(), 0);

    fill(&manager, 5, FILLER);
    h.scheduler.schedule_if_needed(0, h.rx.clone());
    h.scheduler.schedule_if_needed(0, h.rx.clone());
    advance(901).await;
    settle().await;

    assert_eq!(summarizer.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn the_stability_delay_holds_the_summarizer_back() {
    let manager = manager(test_config());
    let summarizer = Arc::new(FakeSummarizer::with_responses(vec![ok("Genre: drama")]));
    let h = harness(manager.clone(), summarizer.clone(), 0);

    fill(&manager, 5, FILLER);
    h.scheduler.schedule_if_needed(0, h.rx.clone());
    advance(899).await;
    settle().await;
    assert_eq!(summarizer.calls(), 0, "900ms must pass before the drain");

    advance(2).await;
    settle().await;
    assert_eq!(summarizer.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_generation_change_during_the_delay_skips_before_the_drain() {
    let manager = manager(test_config());
    let summarizer = Arc::new(FakeSummarizer::with_responses(vec![ok("Genre: drama")]));
    let h = harness(manager.clone(), summarizer.clone(), 0);

    fill(&manager, 5, FILLER);
    h.scheduler.schedule_if_needed(0, h.rx.clone());
    advance(100).await;
    h.generation.fetch_add(1, Ordering::SeqCst);
    advance(801).await;
    settle().await;

    assert_eq!(summarizer.calls(), 0);
    let (used, _budget) = manager.context_usage();
    assert_eq!(used, FIVE_LINES_TOKENS, "history must not be drained");
    assert!(manager.needs_context_compression());
}

#[tokio::test(start_paused = true)]
async fn a_stop_before_work_skips_without_draining() {
    let manager = manager(test_config());
    let summarizer = Arc::new(FakeSummarizer::with_responses(vec![ok("Genre: drama")]));
    let h = harness(manager.clone(), summarizer.clone(), 0);

    fill(&manager, 5, FILLER);
    h.scheduler.schedule_if_needed(0, h.rx.clone());
    let _ = h.tx.send(true);
    advance(1_000).await;
    settle().await;

    assert_eq!(summarizer.calls(), 0);
    let (used, _budget) = manager.context_usage();
    assert_eq!(used, FIVE_LINES_TOKENS);
}

#[tokio::test(start_paused = true)]
async fn an_empty_drain_batch_reaches_no_summarizer() {
    let manager = manager(test_config_large_lines());
    let summarizer = Arc::new(FakeSummarizer::with_responses(vec![ok("Genre: drama")]));
    let h = harness(manager.clone(), summarizer.clone(), 0);

    // Three lines the drain keeps entirely: nothing is handed to the
    // summarizer and the scheduler returns before calling it.
    fill(&manager, 3, 2_000);
    assert!(manager.needs_context_compression());
    h.scheduler.schedule_if_needed(0, h.rx.clone());
    advance(901).await;
    settle().await;

    assert_eq!(summarizer.calls(), 0);
    let (used, _budget) = manager.context_usage();
    assert_eq!(used, 3 * 503);
}

#[tokio::test]
async fn a_disabled_foundry_summarizer_is_unavailable() {
    let mut config = test_config();
    config.enable_foundry_local = false;
    let summarizer = FoundryContextSummarizer::new(config);

    let result = summarizer.summarize(&["line".to_string()]).await;
    assert!(matches!(result, Err(SummarizerError::Unavailable)));
}

#[tokio::test(start_paused = true)]
async fn an_unavailable_summarizer_restores_and_caps_without_retry() {
    let manager = manager(test_config());
    let summarizer = Arc::new(FakeSummarizer::with_responses(vec![unavailable()]));
    let h = harness(manager.clone(), summarizer.clone(), 0);

    fill(&manager, 5, FILLER);
    h.scheduler.schedule_if_needed(0, h.rx.clone());
    advance(901).await;
    settle().await;

    assert_eq!(summarizer.calls(), 1, "unavailable is not retried");
    let (used, _budget) = manager.context_usage();
    assert_eq!(used, FIVE_LINES_TOKENS, "history restored");
    assert!(!manager.needs_context_compression(), "capped");
}

#[tokio::test(start_paused = true)]
async fn a_successful_summary_updates_memory() {
    let manager = manager(test_config());
    let summarizer = Arc::new(FakeSummarizer::with_responses(vec![ok(
        "Genre: drama. Names: Alice, Bob",
    )]));
    let h = harness(manager.clone(), summarizer.clone(), 0);

    fill(&manager, 5, FILLER);
    h.scheduler.schedule_if_needed(0, h.rx.clone());
    advance(901).await;
    settle().await;

    assert_eq!(summarizer.calls(), 1);
    let prompt = manager.get_context_prompt().expect("memory exists");
    assert!(prompt.contains("Genre: drama"));
    assert!(!manager.needs_context_compression());

    // With the flag cleared nothing is scheduled again.
    h.scheduler.schedule_if_needed(5_000, h.rx.clone());
    advance(901).await;
    settle().await;
    assert_eq!(summarizer.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn an_empty_summary_is_retried_and_can_still_succeed() {
    let manager = manager(test_config());
    let summarizer = Arc::new(FakeSummarizer::with_responses(vec![
        ok(""),
        ok("Genre: drama"),
    ]));
    let h = harness(manager.clone(), summarizer.clone(), 0);

    fill(&manager, 5, FILLER);
    h.scheduler.schedule_if_needed(0, h.rx.clone());
    advance(901).await;
    settle().await;
    assert_eq!(summarizer.calls(), 1);

    advance(501).await;
    settle().await;
    assert_eq!(summarizer.calls(), 2);
    let prompt = manager.get_context_prompt().expect("memory exists");
    assert!(prompt.contains("Genre: drama"));
}

#[tokio::test(start_paused = true)]
async fn a_transient_failure_is_retried_and_can_still_succeed() {
    let manager = manager(test_config());
    let summarizer = Arc::new(FakeSummarizer::with_responses(vec![
        failed(),
        ok("Genre: drama"),
    ]));
    let h = harness(manager.clone(), summarizer.clone(), 0);

    fill(&manager, 5, FILLER);
    h.scheduler.schedule_if_needed(0, h.rx.clone());
    advance(901).await;
    settle().await;
    assert_eq!(summarizer.calls(), 1);

    advance(501).await;
    settle().await;
    assert_eq!(summarizer.calls(), 2);
    let prompt = manager.get_context_prompt().expect("memory exists");
    assert!(prompt.contains("Genre: drama"));
}

#[tokio::test(start_paused = true)]
async fn a_terminal_failure_restores_history_and_caps_the_budget() {
    let manager = manager(test_config());
    let summarizer = Arc::new(FakeSummarizer::with_responses(vec![
        failed(),
        failed(),
        failed(),
    ]));
    let h = harness(manager.clone(), summarizer.clone(), 0);

    fill(&manager, 5, FILLER);
    h.scheduler.schedule_if_needed(0, h.rx.clone());
    advance(901).await;
    settle().await;
    assert_eq!(summarizer.calls(), 1);
    advance(501).await;
    settle().await;
    assert_eq!(summarizer.calls(), 2);
    advance(501).await;
    settle().await;

    assert_eq!(summarizer.calls(), 3, "exactly three attempts");
    let (used, _budget) = manager.context_usage();
    assert_eq!(used, FIVE_LINES_TOKENS, "history restored");
    assert!(!manager.needs_context_compression(), "capped");
}

#[tokio::test(start_paused = true)]
async fn a_stop_during_the_retry_keeps_the_history_drained() {
    let manager = manager(test_config());
    let summarizer = Arc::new(FakeSummarizer::with_responses(vec![
        failed(),
        failed(),
        failed(),
    ]));
    let h = harness(manager.clone(), summarizer.clone(), 0);

    fill(&manager, 5, FILLER);
    h.scheduler.schedule_if_needed(0, h.rx.clone());
    advance(901).await;
    settle().await;
    assert_eq!(summarizer.calls(), 1);

    let _ = h.tx.send(true);
    advance(501).await;
    settle().await;

    assert_eq!(summarizer.calls(), 1, "stop interrupts the retry");
    let (used, _budget) = manager.context_usage();
    assert_eq!(used, DRAINED_TOKENS, "no restore on stop, as before");
}

#[tokio::test(start_paused = true)]
async fn a_panicking_summarizer_releases_the_in_flight_flag() {
    let manager = manager(test_config());
    let summarizer = Arc::new(PanicSummarizer {
        calls: AtomicUsize::new(0),
    });
    let h = harness(manager.clone(), summarizer.clone(), 0);

    fill(&manager, 5, FILLER);
    h.scheduler.schedule_if_needed(0, h.rx.clone());
    advance(901).await;
    settle().await;
    assert_eq!(summarizer.calls.load(Ordering::SeqCst), 1);

    // The panicked task released the flag on unwind, so a second schedule
    // must be able to spawn again.
    fill(&manager, 3, FILLER);
    assert!(manager.needs_context_compression());
    h.scheduler.schedule_if_needed(0, h.rx.clone());
    advance(901).await;
    settle().await;
    assert_eq!(summarizer.calls.load(Ordering::SeqCst), 2);
}

// One fresh summarizer per scheduled run preserves the old per-run service
// re-detection; this pins the injected factory is called once per run.
#[tokio::test(start_paused = true)]
async fn each_scheduled_run_gets_a_fresh_summarizer() {
    let manager = manager(test_config());
    let first = Arc::new(FakeSummarizer::with_responses(vec![ok("Genre: drama")]));
    let second = Arc::new(FakeSummarizer::with_responses(vec![ok("Genre: comedy")]));
    let creations = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = watch::channel(false);
    let generation = Arc::new(AtomicU64::new(0));
    let scheduler = ContextCompressionScheduler::new(
        Arc::clone(&manager),
        {
            let creations = Arc::clone(&creations);
            let first = Arc::clone(&first) as Arc<dyn ContextSummarizer>;
            let second = Arc::clone(&second) as Arc<dyn ContextSummarizer>;
            move || {
                let made = creations.fetch_add(1, Ordering::SeqCst) + 1;
                if made == 1 {
                    Arc::clone(&first)
                } else {
                    Arc::clone(&second)
                }
            }
        },
        generation,
        0,
    );

    fill(&manager, 5, FILLER);
    scheduler.schedule_if_needed(0, rx.clone());
    advance(901).await;
    settle().await;

    fill(&manager, 2, FILLER);
    assert!(manager.needs_context_compression());
    scheduler.schedule_if_needed(5_000, rx.clone());
    advance(901).await;
    settle().await;

    assert_eq!(creations.load(Ordering::SeqCst), 2);
    assert_eq!(first.calls(), 1);
    assert_eq!(second.calls(), 1);
    drop(tx);
}

#[test]
fn the_guard_releases_the_flag_on_drop() {
    let flag = Arc::new(std::sync::atomic::AtomicBool::new(true));
    drop(CompressionFlagGuard::new(Arc::clone(&flag)));
    assert!(!flag.load(Ordering::SeqCst));
}
#[test]
fn the_guard_releases_the_flag_when_the_task_panics() {
    let flag = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let guarded = Arc::clone(&flag);
    let panicked = std::panic::catch_unwind(move || {
        let _guard = CompressionFlagGuard::new(guarded);
        panic!("scheduler task fell over");
    })
    .is_err();
    assert!(panicked, "the fixture must actually panic");
    assert!(!flag.load(Ordering::SeqCst));
}
