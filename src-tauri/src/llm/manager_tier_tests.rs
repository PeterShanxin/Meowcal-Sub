// Manager-level characterization of the context-tier progression: the
// degradation chain reaches the fallback, and a degraded tier persists across
// translations. Shared fixtures come from the sibling `tests` module.
use super::tests::{base_config, TestBackend};
use super::*;
use crate::llm::translation_attempt::test_fixtures::{ScriptedBackend, ScriptedStep};
use std::sync::{Arc, Mutex};

// Wave-4 characterization: when every context tier times out, the chain must
// still reach the Mock fallback, and the warnings tell the whole story.
#[tokio::test(start_paused = true)]
async fn all_context_tiers_time_out_and_the_chain_falls_back_to_mock() {
    let mut config = base_config();
    config.foundry_local.timeout_ms = 100;
    let backends: Vec<Box<dyn TranslatorBackend>> = vec![
        Box::new(TestBackend {
            id: BackendId::FoundryLocal,
            available: true,
            ready_state: ReadyState::Ready,
            response: Ok("slow".to_string()),
            delay_ms: 60_000,
        }),
        Box::new(TestBackend {
            id: BackendId::Mock,
            available: true,
            ready_state: ReadyState::Ready,
            response: Ok("你好".to_string()),
            delay_ms: 0,
        }),
    ];

    let diagnostics = Arc::new(Mutex::new(TranslationDiagnosticsState::default()));
    let manager = TranslationManager::with_backends(config, backends, diagnostics, 500);
    // Give the tiers context to degrade through: without a memory prompt the
    // MemoryOnly tier runs uncontexted and breaks out early.
    manager.update_context_memory("memory".to_string());

    let outcome = manager
        .translate_with_context("你好", "zh-CN", "en-US", Some("session context"))
        .await;

    assert_eq!(outcome.backend_used, BackendId::Mock);
    assert_eq!(outcome.translated, "你好");
    assert_eq!(
        outcome.display_state,
        TranslationDisplayState::TemporarilyUnavailable
    );
    let degraded = outcome
        .warnings
        .iter()
        .filter(|warning| **warning == "foundry_local: context_degraded")
        .count();
    assert_eq!(
        degraded, 2,
        "Full and MemoryOnly each degrade once: {:?}",
        outcome.warnings
    );
}

// Wave-4 characterization: the tier degraded by one translation stays degraded
// for the next - the planner reads the stored tier at sequence start. Proved by
// the successful attempt of the second run carrying no context prompt (the
// None tier) instead of the memory prompt (the unpersisted Full tier).
#[tokio::test(start_paused = true)]
async fn a_degraded_tier_persists_across_translations() {
    let mut config = base_config();
    config.foundry_local.timeout_ms = 100;
    let backend = ScriptedBackend::new(
        BackendId::FoundryLocal,
        vec![
            ScriptedStep::Hang,
            ScriptedStep::Ok("first".to_string()),
            ScriptedStep::Hang,
            ScriptedStep::Ok("second".to_string()),
        ],
    );
    let observed = Arc::new(backend.clone());
    let backends: Vec<Box<dyn TranslatorBackend>> = vec![Box::new(backend)];

    let diagnostics = Arc::new(Mutex::new(TranslationDiagnosticsState::default()));
    let manager = TranslationManager::with_backends(config, backends, diagnostics, 500);
    manager.update_context_memory("memory".to_string());

    let first = manager
        .translate_with_context("你好", "zh-CN", "en-US", Some("session context"))
        .await;
    assert_eq!(first.backend_used, BackendId::FoundryLocal);
    assert_eq!(first.translated, "first");
    let first_degraded = first
        .warnings
        .iter()
        .filter(|warning| **warning == "foundry_local: context_degraded")
        .count();
    assert_eq!(
        first_degraded, 1,
        "Full times out once, then MemoryOnly answers: {:?}",
        first.warnings
    );

    let second = manager
        .translate_with_context("你好", "zh-CN", "en-US", Some("session context"))
        .await;
    assert_eq!(second.backend_used, BackendId::FoundryLocal);
    assert_eq!(second.translated, "second");
    let second_degraded = second
        .warnings
        .iter()
        .filter(|warning| **warning == "foundry_local: context_degraded")
        .count();
    assert_eq!(
        second_degraded, 1,
        "the second run starts at the tier the first run stored: {:?}",
        second.warnings
    );
    let seen = lock_or_recover(&observed.options_seen);
    assert_eq!(seen.len(), 4);
    assert!(
        seen[3]
            .as_ref()
            .is_none_or(|options| !options.enable_context),
        "the persisted tier is None by the second run's success: {:?}",
        seen[3]
    );
}

// The two caps are set in different modules and neither compiles against
// the other, so nothing but this test stops them drifting back apart. At
// 6500 against a 5000 deadline the whole fallback chain below - retry,
// context degradation, Mock source-passthrough - was unreachable for anyone
// running without context-aware translation.
