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
    assert!(!errors.contains_key("foundry_local"));
    assert!(latencies.contains_key("foundry_local"));
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
    assert!(!errors.contains_key("foundry_local"));
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
        errors.get("foundry_local").map(String::as_str),
        Some("api_error")
    );
    assert!(latencies.contains_key("foundry_local"));
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
        errors.get("foundry_local").map(String::as_str),
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
        errors.get("foundry_local").map(String::as_str),
        Some("low_quality_output")
    );
}
