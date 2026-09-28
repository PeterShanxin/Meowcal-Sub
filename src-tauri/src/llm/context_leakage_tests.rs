use crate::config::{ContextLevel, TranslationConfig};
use crate::llm::translation_attempt::test_fixtures::{ScriptedBackend, ScriptedStep};
use crate::llm::{
    BackendId, TranslationDiagnosticsState, TranslationDisplayState, TranslationManager,
};
use crate::sync_utils::lock_or_recover;
use std::sync::{Arc, Mutex};

fn manager(answers: &[&str], enabled: bool) -> (TranslationManager, ScriptedBackend) {
    let config = TranslationConfig {
        enable_context_aware: enabled,
        context_level: if enabled {
            ContextLevel::MemoryAndRecent
        } else {
            ContextLevel::Off
        },
        ..TranslationConfig::default()
    };
    manager_with_config(answers, config)
}

fn manager_with_config(
    answers: &[&str],
    config: TranslationConfig,
) -> (TranslationManager, ScriptedBackend) {
    manager_with_steps(
        answers
            .iter()
            .map(|answer| ScriptedStep::Ok((*answer).into()))
            .collect(),
        config,
    )
}

fn manager_with_steps(
    steps: Vec<ScriptedStep>,
    config: TranslationConfig,
) -> (TranslationManager, ScriptedBackend) {
    let backend = ScriptedBackend::new(BackendId::FoundryLocal, steps);
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
async fn context_leakage_recovers_a_replay_rejected_by_the_length_validator() {
    let previous = "房间是空的。";
    let replay = format!(
        "{previous}{}",
        "我们只有十分钟，必须立刻离开这里。".repeat(3)
    );
    let rejection = crate::llm::output_validation::validate_translation_output(
        "Not yet.",
        &replay,
        "en",
        "zh",
        crate::translation_eligibility::Eligibility::SubtitleLike,
    );
    assert_eq!(
        rejection,
        Err(crate::llm::output_validation::TranslationOutputRejection::TooLong)
    );
    let (manager, backend) = manager(&[previous, &replay, "还没有。"], true);
    manager
        .translate_with_fallback("The room is empty.", "en", "zh")
        .await;
    let result = manager
        .translate_with_context("Not yet.", "en", "zh", Some("The room is empty."))
        .await;
    assert_eq!(result.translated, "还没有。");
    assert_eq!(result.display_state, TranslationDisplayState::Translated);
    assert_eq!(context_flags(&backend), vec![false, true, false]);
}

#[tokio::test]
async fn context_leakage_recovers_replay_rejected_by_managed_prevalidation() {
    let previous = "房间是空的。";
    let replay = format!(
        "{previous}{}",
        "我们只有十分钟，必须立刻离开这里。".repeat(3)
    );
    let (manager, backend) = manager_with_steps(
        [previous, &replay, "还没有。"]
            .iter()
            .map(|answer| ScriptedStep::ManagedOk((*answer).into()))
            .collect(),
        TranslationConfig {
            enable_context_aware: true,
            context_level: ContextLevel::MemoryAndRecent,
            ..TranslationConfig::default()
        },
    );
    manager
        .translate_with_fallback("The room is empty.", "en", "zh")
        .await;
    let result = manager
        .translate_with_context("Not yet.", "en", "zh", Some("The room is empty."))
        .await;
    assert_eq!(result.translated, "还没有。");
    assert_eq!(result.display_state, TranslationDisplayState::Translated);
    assert_eq!(context_flags(&backend), vec![false, true, false]);
}

#[tokio::test]
async fn context_leakage_managed_rejection_without_replay_does_not_retry() {
    let output = "我们只有十分钟，必须立刻离开这里。".repeat(3);
    let (manager, backend) = manager_with_steps(
        vec![
            ScriptedStep::ManagedOk("房间是空的。".into()),
            ScriptedStep::ManagedOk(output),
        ],
        TranslationConfig {
            enable_context_aware: true,
            context_level: ContextLevel::MemoryAndRecent,
            ..TranslationConfig::default()
        },
    );
    manager
        .translate_with_fallback("The room is empty.", "en", "zh")
        .await;
    let result = manager
        .translate_with_context("Not yet.", "en", "zh", Some("The room is empty."))
        .await;
    assert_ne!(result.display_state, TranslationDisplayState::Translated);
    assert_eq!(context_flags(&backend), vec![false, true]);
    assert!(!result
        .warnings
        .iter()
        .any(|warning| warning.contains("context_leakage")));
}

#[tokio::test]
async fn context_leakage_short_cjk_fragments_preserve_context() {
    let (manager, backend) = manager(&["是", "是的", "还没有。"], true);
    manager.translate_with_fallback("It is.", "en", "zh").await;
    let result = manager
        .translate_with_context("Agreed.", "en", "zh", Some("It is."))
        .await;
    assert_eq!(result.translated, "是的");
    manager
        .translate_with_context("Not yet.", "en", "zh", Some("Agreed."))
        .await;
    assert_eq!(context_flags(&backend), vec![false, true, true]);
}

#[tokio::test]
async fn context_leakage_scene_gap_expires_previous_translation_evidence() {
    let config = TranslationConfig {
        enable_context_aware: true,
        context_level: ContextLevel::MemoryAndRecent,
        context_reset_gap_ms: 6_000,
        ..TranslationConfig::default()
    };
    let (manager, backend) = manager_with_config(&["还没有。", "还没有。", "以后再说。"], config);
    manager.restore_history_entries(vec![crate::llm::HistoryEntry {
        text: "Not yet.".into(),
        timestamp: std::time::Instant::now() - std::time::Duration::from_secs(60),
        token_estimate: 3,
    }]);
    manager
        .translate_with_fallback("Not yet.", "en", "zh")
        .await;
    // The capture loop builds the prompt before it records the next OCR line.
    assert!(manager.get_context_prompt().is_none());
    manager.record_ocr_line("We should wait.");
    let context = manager.get_context_prompt().unwrap();
    assert!(!context.contains("Not yet."));
    let result = manager
        .translate_with_context("Not now.", "en", "zh", Some(&context))
        .await;
    assert_eq!(result.translated, "还没有。");
    assert_eq!(context_flags(&backend), vec![false, true]);
}

#[tokio::test]
async fn context_leakage_reset_ignores_an_in_flight_previous_scene() {
    let release = Arc::new(tokio::sync::Notify::new());
    let backend = ScriptedBackend::new(
        BackendId::FoundryLocal,
        vec![
            ScriptedStep::Wait(release.clone(), "还没有。".into()),
            ScriptedStep::Ok("还没有。".into()),
            ScriptedStep::Ok("以后再说。".into()),
        ],
    );
    let config = TranslationConfig {
        enable_context_aware: true,
        context_level: ContextLevel::MemoryAndRecent,
        ..TranslationConfig::default()
    };
    let manager = TranslationManager::with_backends(
        config,
        vec![Box::new(backend.clone())],
        Arc::new(Mutex::new(TranslationDiagnosticsState::default())),
        500,
    );
    let stale = manager.translate_with_fallback("Not yet.", "en", "zh");
    tokio::pin!(stale);
    tokio::select! {
        biased;
        _ = &mut stale => panic!("previous scene must wait for release"),
        _ = tokio::task::yield_now() => {}
    }
    assert_eq!(context_flags(&backend), vec![false]);
    manager.reset_context();
    release.notify_one();
    assert_eq!(stale.await.translated, "还没有。");
    let result = manager
        .translate_with_context("Not now.", "en", "zh", Some("New scene"))
        .await;
    assert_eq!(result.translated, "还没有。");
    assert_eq!(context_flags(&backend), vec![false, true]);
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
