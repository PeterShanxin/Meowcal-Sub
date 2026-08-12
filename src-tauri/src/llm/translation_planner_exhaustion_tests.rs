// Exhausted/terminal paths of the context-tier progression: all tiers timing
// out, a validation rejection ending the sequence, a spent budget refusing at
// entry, the slow-success degrade, and the diagnostics side of a success.
// Shared fakes come from the sibling `tests` module.
use super::tests::{diagnostics, plan, tier_store, RecordingBackend, StepOutcome};
use super::*;
use crate::llm::translation_attempt::test_fixtures::{
    budget, default_policy, ScriptedBackend, ScriptedStep,
};
use crate::llm::{BackendId, LlmError, TranslationDiagnosticsState};
use crate::sync_utils::lock_or_recover;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};
use tokio::time::Duration;

// Every tier times out: Full and MemoryOnly degrade once each, the None tier
// exhausts its uncontexted retries, and the sequence ends for the next backend.
#[tokio::test(start_paused = true)]
async fn all_tiers_timeout_then_the_sequence_exhausts() {
    let backend = Arc::new(ScriptedBackend::new(
        BackendId::FoundryLocal,
        vec![ScriptedStep::Hang],
    ));
    let store = tier_store(ContextTier::Full);
    let planner = TranslationPlanner::new(default_policy(3), diagnostics());
    let mut warnings = Vec::new();

    let outcome = planner
        .run_tiered_sequence(
            backend.as_ref(),
            &plan(&store, ContextTier::Full, Some("FULL"), Some("MEM")),
            ReadyState::Ready,
            &budget(10_000),
            &mut warnings,
        )
        .await;

    assert!(outcome.is_none(), "all tiers failed: {outcome:?}");
    assert_eq!(
        warnings,
        vec![
            "foundry_local: timeout".to_string(),
            "foundry_local: context_degraded".to_string(),
            "foundry_local: timeout".to_string(),
            "foundry_local: context_degraded".to_string(),
            "foundry_local: timeout".to_string(),
        ]
    );
    assert_eq!(
        ContextTier::from_u8(store.load(Ordering::SeqCst)),
        ContextTier::None,
        "the stored tier ends at the bottom of the ladder"
    );
    assert_eq!(
        backend.calls.load(Ordering::SeqCst),
        5,
        "1 contexted + 1 contexted + 3 uncontexted retries"
    );
}

// A deterministically rejected output is terminal at the tier: one call, the
// sequence gives up for the next backend, and the diagnostics record the
// quality rejection.
#[tokio::test]
async fn a_validation_rejection_exhausts_the_sequence_without_retry() {
    let backend = Arc::new(ScriptedBackend::new(
        BackendId::FoundryLocal,
        vec![ScriptedStep::Ok("a".repeat(150))],
    ));
    let store = tier_store(ContextTier::Full);
    let diagnostics = diagnostics();
    let planner = TranslationPlanner::new(default_policy(3), diagnostics.clone());
    let mut warnings = Vec::new();

    let outcome = planner
        .run_tiered_sequence(
            backend.as_ref(),
            &plan(&store, ContextTier::Full, Some("FULL"), Some("MEM")),
            ReadyState::Ready,
            &budget(10_000),
            &mut warnings,
        )
        .await;

    assert!(outcome.is_none(), "a rejected output must not succeed");
    assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings[0].starts_with("foundry_local: Translation failed: ")
            && warnings[0].contains("rejected as corrupted (overlong output)"),
        "the rejection reason reaches the warnings: {:?}",
        warnings
    );
    let (errors, _) = lock_or_recover(&diagnostics).snapshot();
    assert_eq!(
        errors.get("foundry_local").map(String::as_str),
        Some("low_quality_output")
    );
}

// No budget at all: the attempt runner refuses at entry, and the sequence
// hands the line to the next backend without calling anything.
#[tokio::test]
async fn an_exhausted_budget_at_entry_never_calls_the_backend() {
    let backend = Arc::new(ScriptedBackend::new(
        BackendId::FoundryLocal,
        vec![ScriptedStep::Hang],
    ));
    let store = tier_store(ContextTier::Full);
    let planner = TranslationPlanner::new(default_policy(3), diagnostics());
    let mut warnings = Vec::new();

    let outcome = planner
        .run_tiered_sequence(
            backend.as_ref(),
            &plan(&store, ContextTier::Full, Some("FULL"), Some("MEM")),
            ReadyState::Ready,
            &budget(0),
            &mut warnings,
        )
        .await;

    assert!(outcome.is_none(), "nothing can run: {outcome:?}");
    assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
    assert_eq!(warnings, vec!["foundry_local: timeout".to_string()]);
}

// A success writes the diagnostics success key from the shared clock.
#[tokio::test]
async fn a_success_records_diagnostics_from_the_shared_clock() {
    let backend = Arc::new(ScriptedBackend::new(
        BackendId::FoundryLocal,
        vec![ScriptedStep::Ok("hello world".to_string())],
    ));
    let store = tier_store(ContextTier::Full);
    let diagnostics = diagnostics();
    let planner = TranslationPlanner::new(default_policy(3), diagnostics.clone());
    let mut warnings = Vec::new();

    let outcome = planner
        .run_tiered_sequence(
            backend.as_ref(),
            &plan(&store, ContextTier::Full, Some("FULL"), Some("MEM")),
            ReadyState::Ready,
            &budget(10_000),
            &mut warnings,
        )
        .await
        .expect("the first tier succeeds");

    assert_eq!(outcome.translated, "hello world");
    let (errors, latencies) = lock_or_recover(&diagnostics).snapshot();
    assert!(
        errors.get("foundry_local").is_none(),
        "a success clears the last error"
    );
    assert!(
        latencies.get("foundry_local").is_some(),
        "the success latency reaches the diagnostics"
    );
}
// A slow-but-successful answer degrades the stored tier for future requests,
// with the threshold applied to the shared budget clock.
#[tokio::test]
async fn a_slow_success_degrades_the_stored_tier() {
    let backend = RecordingBackend {
        script: vec![StepOutcome {
            delay_ms: 100,
            response: Ok("hello world".to_string()),
        }],
        calls: AtomicUsize::new(0),
        seen: Mutex::new(Vec::new()),
    };
    let store = tier_store(ContextTier::Full);
    let planner = TranslationPlanner::with_slow_degrade_ms(default_policy(3), diagnostics(), 50);
    let mut warnings = Vec::new();

    let outcome = planner
        .run_tiered_sequence(
            &backend,
            &plan(&store, ContextTier::Full, Some("FULL"), Some("MEM")),
            ReadyState::Ready,
            &budget(10_000),
            &mut warnings,
        )
        .await
        .expect("the slow answer still succeeds");

    assert_eq!(outcome.translated, "hello world");
    assert_eq!(
        warnings,
        vec!["foundry_local: context_degraded_slow".to_string()]
    );
    assert_eq!(
        ContextTier::from_u8(store.load(Ordering::SeqCst)),
        ContextTier::MemoryOnly,
        "the slow line degrades the effective tier"
    );
}
