use crate::config::{ContextLevel, TranslationConfig};
use crate::llm::translation_attempt::test_fixtures::{ScriptedBackend, ScriptedStep};
use crate::llm::{
    BackendId, TranslationDiagnosticsState, TranslationDisplayState, TranslationManager,
};
use crate::sync_utils::lock_or_recover;
use std::sync::{Arc, Mutex};

fn manager(answers: &[&str], enabled: bool) -> (TranslationManager, ScriptedBackend) {
    let backend = ScriptedBackend::new(
        BackendId::FoundryLocal,
        answers
            .iter()
            .map(|answer| ScriptedStep::Ok((*answer).into()))
            .collect(),
    );
    let config = TranslationConfig {
        enable_context_aware: enabled,
        context_level: if enabled {
            ContextLevel::MemoryAndRecent
        } else {
            ContextLevel::Off
        },
        ..TranslationConfig::default()
    };
    let manager = TranslationManager::with_backends(
        config,
        vec![Box::new(backend.clone())],
        Arc::new(Mutex::new(TranslationDiagnosticsState::default())),
        500,
    );
    (manager, backend)
}

fn context_flags(backend: &ScriptedBackend) -> Vec<bool> {
    lock_or_recover(&backend.options_seen)
        .iter()
        .map(|options| {
            options
                .as_ref()
                .is_some_and(|options| options.enable_context)
        })
        .collect()
}

#[tokio::test]
async fn context_leakage_retries_without_context_and_persists_safe_tier() {
    let (manager, backend) = manager(
        &["房间是空的。", "房间是空的。", "还没有。", "走吧。"],
        true,
    );
    manager
        .translate_with_fallback("The room is empty.", "en", "zh")
        .await;
    let result = manager
        .translate_with_context("Not yet.", "en", "zh", Some("The room is empty."))
        .await;
    assert_eq!(result.translated, "还没有。");
    assert_eq!(result.display_state, TranslationDisplayState::Translated);
    assert!(result
        .warnings
        .iter()
        .any(|warning| warning == "local_engine: context_leakage"));
    manager
        .translate_with_context("Let's go.", "en", "zh", Some("Not yet."))
        .await;
    assert_eq!(context_flags(&backend), vec![false, true, false, false]);
}

#[tokio::test]
async fn context_leakage_rejects_a_previous_translation_prepended_to_the_current_one() {
    let (manager, backend) = manager(
        &["房间是空的。", "房间是空的，还有十分钟。", "还有十分钟。"],
        true,
    );
    manager
        .translate_with_fallback("The room is empty.", "en", "zh")
        .await;
    let result = manager
        .translate_with_context(
            "We have ten minutes.",
            "en",
            "zh",
            Some("The room is empty."),
        )
        .await;
    assert_eq!(result.translated, "还有十分钟。");
    assert_eq!(context_flags(&backend), vec![false, true, false]);
}

#[tokio::test]
async fn context_leakage_allows_an_unchanged_source_and_ordinary_context_success() {
    let (manager, backend) = manager(&["房间是空的。", "房间是空的。", "还有十分钟。"], true);
    manager
        .translate_with_fallback("The room is empty.", "en", "zh")
        .await;
    let repeated = manager
        .translate_with_context("The room is empty!", "en", "zh", Some("The room is empty."))
        .await;
    assert_eq!(repeated.translated, "房间是空的。");
    let result = manager
        .translate_with_context(
            "We have ten minutes.",
            "en",
            "zh",
            Some("The room is empty."),
        )
        .await;
    assert_eq!(result.translated, "还有十分钟。");
    assert_eq!(context_flags(&backend), vec![false, true, true]);
}

#[tokio::test]
async fn context_leakage_retry_can_confirm_a_legitimately_shared_translation() {
    let (manager, backend) = manager(&["还没有。", "还没有。", "还没有。"], true);
    manager
        .translate_with_fallback("Not yet.", "en", "zh")
        .await;
    let result = manager
        .translate_with_context("Not now.", "en", "zh", Some("Not yet."))
        .await;
    assert_eq!(result.translated, "还没有。");
    assert_eq!(result.display_state, TranslationDisplayState::Translated);
    assert_eq!(context_flags(&backend), vec![false, true, false]);
}

#[tokio::test]
async fn context_leakage_failed_retry_never_returns_the_contaminated_answer() {
    let (manager, backend) = manager(&["房间是空的。", "房间是空的。", ""], true);
    manager
        .translate_with_fallback("The room is empty.", "en", "zh")
        .await;
    let result = manager
        .translate_with_context("Not yet.", "en", "zh", Some("The room is empty."))
        .await;
    assert_ne!(result.display_state, TranslationDisplayState::Translated);
    assert_ne!(result.translated, "房间是空的。");
    assert_eq!(context_flags(&backend), vec![false, true, false]);
}

#[tokio::test]
async fn context_leakage_disabled_context_does_not_retry_shared_output() {
    let (manager, backend) = manager(&["还没有。", "还没有。"], false);
    manager
        .translate_with_fallback("Not yet.", "en", "zh")
        .await;
    let result = manager
        .translate_with_context("Not now.", "en", "zh", Some("Not yet."))
        .await;
    assert_eq!(result.translated, "还没有。");
    assert_eq!(context_flags(&backend), vec![false, false]);
}

#[tokio::test]
async fn context_leakage_reset_and_language_pair_do_not_reuse_other_history() {
    let (manager, backend) = manager(&["还没有。", "还没有。", "还没有。"], true);
    manager
        .translate_with_fallback("Not yet.", "en", "zh")
        .await;
    manager
        .translate_with_context("まだです", "ja", "zh", Some("Previous dialogue"))
        .await;
    manager.reset_context();
    manager
        .translate_with_context("Not now.", "en", "zh", Some("New session"))
        .await;
    assert_eq!(context_flags(&backend), vec![false, true, true]);
}
