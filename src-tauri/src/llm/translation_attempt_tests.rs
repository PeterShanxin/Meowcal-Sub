use super::test_fixtures::*;
use super::*;
use crate::llm::BackendId;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn a_first_attempt_success_is_returned_with_success_diagnostics() {
    let (runner, backend, diagnostics) = harness(vec![ScriptedStep::Ok("hello world".to_string())]);
    let mut warnings = Vec::new();

    let outcome = runner
        .run(
            backend.as_ref(),
            &zh_request("你好", Some("session context"), true),
            &budget(10_000),
            ReadyState::Ready,
            &mut warnings,
        )
        .await;

    let (translated, recovered) = expect_succeeded(outcome);
    assert_eq!(translated, "hello world");
    assert!(
        !recovered,
        "a first-attempt success is not recovered-after-retry"
    );
    assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    assert!(warnings.is_empty());

    let (errors, latencies) = lock_or_recover(&diagnostics).snapshot();
    assert!(!errors.contains_key("local_engine"));
    assert!(latencies.contains_key("local_engine"));
}

#[tokio::test(start_paused = true)]
async fn a_transient_error_is_retried_with_the_scaled_delay_and_then_succeeds() {
    let (runner, backend, diagnostics) = harness(vec![
        ScriptedStep::Err(LlmError::ApiError("connection refused".to_string())),
        ScriptedStep::Ok("hello world".to_string()),
    ]);
    let mut warnings = Vec::new();

    let outcome = runner
        .run(
            backend.as_ref(),
            &zh_request("你好", None, true),
            &budget(10_000),
            ReadyState::Ready,
            &mut warnings,
        )
        .await;

    let (translated, recovered) = expect_succeeded(outcome);
    assert_eq!(translated, "hello world");
    assert!(
        recovered,
        "the second attempt recovered after the first failed"
    );
    assert_eq!(backend.calls.load(Ordering::SeqCst), 2);
    assert!(warnings.is_empty());

    let times = lock_or_recover(&backend.virtual_call_times);
    assert_eq!(times.len(), 2);
    assert_eq!(times[1] - times[0], Duration::from_millis(600));

    let (errors, _) = lock_or_recover(&diagnostics).snapshot();
    assert!(!errors.contains_key("local_engine"));
}

#[tokio::test(start_paused = true)]
async fn transient_errors_exhaust_the_retry_count_with_scaled_delays() {
    let (runner, backend, diagnostics) = harness(vec![ScriptedStep::Err(LlmError::ApiError(
        "connection refused".to_string(),
    ))]);
    let mut warnings = Vec::new();

    let outcome = runner
        .run(
            backend.as_ref(),
            &zh_request("你好", None, true),
            &budget(10_000),
            ReadyState::Ready,
            &mut warnings,
        )
        .await;

    let err = expect_failed(outcome);
    assert_eq!(err.code(), "api_error");
    assert_eq!(backend.calls.load(Ordering::SeqCst), 3);
    assert!(
        warnings.is_empty(),
        "the Failed warning belongs to the tier loop"
    );

    let times = lock_or_recover(&backend.virtual_call_times);
    assert_eq!(times.len(), 3);
    assert_eq!(times[1] - times[0], Duration::from_millis(600));
    assert_eq!(times[2] - times[1], Duration::from_millis(1200));

    let (errors, latencies) = lock_or_recover(&diagnostics).snapshot();
    assert_eq!(
        errors.get("local_engine").map(String::as_str),
        Some("api_error")
    );
    assert!(latencies.contains_key("local_engine"));
}

#[tokio::test]
async fn a_non_transient_error_is_not_retried() {
    let (runner, backend, diagnostics) = harness(vec![ScriptedStep::Err(LlmError::ApiError(
        "API error 404 Not Found".to_string(),
    ))]);
    let mut warnings = Vec::new();

    let outcome = runner
        .run(
            backend.as_ref(),
            &zh_request("你好", None, true),
            &budget(10_000),
            ReadyState::Ready,
            &mut warnings,
        )
        .await;

    let err = expect_failed(outcome);
    assert_eq!(err.code(), "api_error");
    assert_eq!(backend.calls.load(Ordering::SeqCst), 1);

    let (errors, _) = lock_or_recover(&diagnostics).snapshot();
    assert_eq!(
        errors.get("local_engine").map(String::as_str),
        Some("api_error")
    );
}

#[tokio::test]
async fn a_rejected_output_fails_without_retry_and_keeps_the_quality_code() {
    let (runner, backend, diagnostics) = harness(vec![
        ScriptedStep::Ok("a".repeat(150)),
        ScriptedStep::Ok("a later translation".to_string()),
    ]);
    let mut warnings = Vec::new();

    let outcome = runner
        .run(
            backend.as_ref(),
            &zh_request("你好", None, true),
            &budget(10_000),
            ReadyState::Ready,
            &mut warnings,
        )
        .await;

    let err = expect_failed(outcome);
    let LlmError::TranslationError(message) = err else {
        panic!("expected TranslationError, got {err:?}");
    };
    assert_eq!(
        message,
        "Translation output rejected as corrupted (overlong output)."
    );
    assert_eq!(
        backend.calls.load(Ordering::SeqCst),
        1,
        "a rejected output is never retried, even with attempts remaining"
    );

    let (errors, _) = lock_or_recover(&diagnostics).snapshot();
    assert_eq!(
        errors.get("local_engine").map(String::as_str),
        Some("low_quality_output")
    );
}

// General text can run far past the source the prompt carries. The output has to
// be judged against what the model was given, or a long page licenses a response
// many times longer than anything it was asked to translate.
#[tokio::test]
async fn output_is_judged_against_the_clipped_source_the_model_saw() {
    let page = "Your storage is almost full. ".repeat(40);
    let rambling: String = (0..2000u32)
        .filter_map(|offset| char::from_u32(0x4E00 + offset))
        .collect();
    let backend = ScriptedBackend::new(BackendId::FoundryLocal, vec![ScriptedStep::Ok(rambling)]);
    let runner = TranslationAttemptRunner::new(
        AttemptPolicy {
            eligibility: crate::translation_eligibility::Eligibility::AnyText,
            ..default_policy(1)
        },
        Arc::new(Mutex::new(TranslationDiagnosticsState::default())),
    );
    let request = AttemptRequest {
        text: &page,
        source_language: "en-US",
        target_language: "zh-CN",
        context_prompt: None,
        context_used: false,
    };
    let mut warnings = Vec::new();

    let outcome = runner
        .run(
            &backend,
            &request,
            &budget(10_000),
            ReadyState::Ready,
            &mut warnings,
        )
        .await;

    let LlmError::TranslationError(message) = expect_failed(outcome) else {
        panic!("expected a rejected translation");
    };
    assert_eq!(
        message,
        "Translation output rejected as corrupted (overlong output)."
    );
}
