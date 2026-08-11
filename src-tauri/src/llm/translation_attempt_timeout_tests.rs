use super::test_fixtures::*;
use super::*;
use crate::llm::BackendId;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

#[tokio::test(start_paused = true)]
async fn an_uncontexted_timeout_retries_without_sleep_and_warns_exactly_once() {
    let (runner, backend, diagnostics) = harness(vec![ScriptedStep::Hang]);
    let mut warnings = Vec::new();

    let outcome = runner
        .run(
            backend.as_ref(),
            &zh_request("你好", None, false),
            &budget(10_000),
            ReadyState::Ready,
            &mut warnings,
        )
        .await;

    assert!(
        !expect_timed_out(outcome),
        "the total budget still has room"
    );
    assert_eq!(backend.calls.load(Ordering::SeqCst), 3);
    assert_eq!(warnings, vec!["foundry_local: timeout".to_string()]);

    let times = lock_or_recover(&backend.virtual_call_times);
    assert_eq!(times.len(), 3);
    assert_eq!(times[1] - times[0], Duration::from_millis(2500));
    assert_eq!(times[2] - times[1], Duration::from_millis(2500));

    let (errors, _) = lock_or_recover(&diagnostics).snapshot();
    assert_eq!(
        errors.get("foundry_local").map(String::as_str),
        Some("timeout")
    );
}

#[tokio::test(start_paused = true)]
async fn a_contexted_timeout_is_not_retried() {
    let (runner, backend, diagnostics) = harness(vec![ScriptedStep::Hang]);
    let mut warnings = Vec::new();

    let outcome = runner
        .run(
            backend.as_ref(),
            &zh_request("你好", Some("ctx"), true),
            &budget(10_000),
            ReadyState::Ready,
            &mut warnings,
        )
        .await;

    assert!(
        !expect_timed_out(outcome),
        "the total budget still has room"
    );
    assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    assert_eq!(warnings, vec!["foundry_local: timeout".to_string()]);

    let (errors, _) = lock_or_recover(&diagnostics).snapshot();
    assert_eq!(
        errors.get("foundry_local").map(String::as_str),
        Some("timeout")
    );
}

#[tokio::test]
async fn an_exhausted_budget_never_calls_the_backend() {
    let (runner, backend, diagnostics) = harness(vec![ScriptedStep::Hang]);
    let mut warnings = Vec::new();

    let outcome = runner
        .run(
            backend.as_ref(),
            &zh_request("你好", None, true),
            &budget(0),
            ReadyState::Ready,
            &mut warnings,
        )
        .await;

    assert!(
        expect_timed_out(outcome),
        "no budget at all is total exhaustion"
    );
    assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
    assert_eq!(warnings, vec!["foundry_local: timeout".to_string()]);

    let (errors, _) = lock_or_recover(&diagnostics).snapshot();
    assert_eq!(
        errors.get("foundry_local").map(String::as_str),
        Some("timeout")
    );
}

#[tokio::test(start_paused = true)]
async fn a_transient_error_without_remaining_delay_room_fails_without_sleeping() {
    let (runner, backend, _diagnostics) = harness(vec![
        ScriptedStep::Err(LlmError::ApiError("connection refused".to_string())),
        ScriptedStep::Ok("hello world".to_string()),
    ]);
    let mut warnings = Vec::new();

    let outcome = runner
        .run(
            backend.as_ref(),
            &zh_request("你好", None, true),
            &budget(500),
            ReadyState::Ready,
            &mut warnings,
        )
        .await;

    let err = expect_failed(outcome);
    assert_eq!(err.code(), "api_error");
    assert_eq!(
        backend.calls.load(Ordering::SeqCst),
        1,
        "the 600ms delay cannot fit in the remaining budget, so no retry"
    );
}

#[tokio::test(start_paused = true)]
async fn the_attempt_cap_honors_whether_context_is_used() {
    let mut policy = default_policy(3);
    policy.contexted_attempt_cap_ms = 400;
    policy.uncontexted_attempt_cap_ms = 800;

    let backend = Arc::new(ScriptedBackend::new(
        BackendId::FoundryLocal,
        vec![ScriptedStep::Hang],
    ));
    let diagnostics = Arc::new(Mutex::new(TranslationDiagnosticsState::default()));
    let runner = TranslationAttemptRunner::new(policy.clone(), diagnostics);
    let mut warnings = Vec::new();
    let attempt_request = zh_request("你好", Some("ctx"), true);
    let attempt_budget = budget(10_000);
    let guarded = tokio::time::timeout(
        Duration::from_millis(600),
        runner.run(
            backend.as_ref(),
            &attempt_request,
            &attempt_budget,
            ReadyState::Ready,
            &mut warnings,
        ),
    );
    assert!(
        matches!(
            guarded.await,
            Ok(AttemptOutcome::TimedOut {
                total_exhausted: false
            })
        ),
        "the contexted attempt must die at its own 400ms cap, not the 800ms one"
    );
    assert_eq!(backend.calls.load(Ordering::SeqCst), 1);

    let backend = Arc::new(ScriptedBackend::new(
        BackendId::FoundryLocal,
        vec![ScriptedStep::Hang],
    ));
    let diagnostics = Arc::new(Mutex::new(TranslationDiagnosticsState::default()));
    let runner = TranslationAttemptRunner::new(policy.clone(), diagnostics);
    let mut warnings = Vec::new();
    let attempt_request = zh_request("你好", None, false);
    let attempt_budget = budget(10_000);
    let too_soon = tokio::time::timeout(
        Duration::from_millis(700),
        runner.run(
            backend.as_ref(),
            &attempt_request,
            &attempt_budget,
            ReadyState::Ready,
            &mut warnings,
        ),
    );
    assert!(
        too_soon.await.is_err(),
        "the uncontexted attempt still runs past 700ms toward its 800ms cap"
    );
    let backend = Arc::new(ScriptedBackend::new(
        BackendId::FoundryLocal,
        vec![ScriptedStep::Hang],
    ));
    let diagnostics = Arc::new(Mutex::new(TranslationDiagnosticsState::default()));
    let runner = TranslationAttemptRunner::new(policy, diagnostics);
    let mut warnings = Vec::new();
    let attempt_request = zh_request("你好", None, false);
    let attempt_budget = budget(10_000);
    let in_time = tokio::time::timeout(
        Duration::from_millis(2500),
        runner.run(
            backend.as_ref(),
            &attempt_request,
            &attempt_budget,
            ReadyState::Ready,
            &mut warnings,
        ),
    );
    assert!(
        matches!(
            in_time.await,
            Ok(AttemptOutcome::TimedOut {
                total_exhausted: false
            })
        ),
        "three 800ms attempts finish well inside 2500ms"
    );
    assert_eq!(backend.calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn prompt_router_options_mirror_the_request_and_policy() {
    let mut policy = default_policy(1);
    policy.prompt_max_context_chars = 777;
    policy.prompt_max_source_chars = 333;
    let backend = Arc::new(ScriptedBackend::new(
        BackendId::FoundryLocal,
        vec![ScriptedStep::Ok("hello world".to_string())],
    ));
    let diagnostics = Arc::new(Mutex::new(TranslationDiagnosticsState::default()));
    let runner = TranslationAttemptRunner::new(policy, diagnostics);
    let mut warnings = Vec::new();

    let outcome = runner
        .run(
            backend.as_ref(),
            &zh_request("你好", Some("ctx"), true),
            &budget(10_000),
            ReadyState::Ready,
            &mut warnings,
        )
        .await;

    let (translated, _) = expect_succeeded(outcome);
    assert_eq!(translated, "hello world");

    let seen = lock_or_recover(&backend.options_seen);
    assert_eq!(seen.len(), 1);
    let Some(options) = seen[0] else {
        panic!("the runner must hand the prompt options to the backend");
    };
    assert!(
        options.enable_context,
        "enable_context mirrors context_used"
    );
    assert_eq!(options.max_context_chars, 777);
    assert_eq!(options.max_source_chars, 333);
}

#[tokio::test]
async fn latency_is_measured_from_the_shared_budget_clock() {
    let (runner, backend, diagnostics) = harness(vec![ScriptedStep::Ok("hello world".to_string())]);
    let mut warnings = Vec::new();
    let started = Instant::now() - Duration::from_secs(5);
    let budget = AttemptBudget {
        started,
        total_timeout: Duration::from_millis(10_000),
    };

    let outcome = runner
        .run(
            backend.as_ref(),
            &zh_request("你好", None, true),
            &budget,
            ReadyState::Ready,
            &mut warnings,
        )
        .await;

    let AttemptOutcome::Succeeded { latency_ms, .. } = outcome else {
        panic!("expected Succeeded, got {outcome:?}");
    };
    assert!(
        latency_ms >= 5_000,
        "latency is measured from the shared budget clock, not from runner entry"
    );
    let (_, latencies) = lock_or_recover(&diagnostics).snapshot();
    assert!(matches!(latencies.get("foundry_local").copied(), Some(v) if v >= 5_000));
}
