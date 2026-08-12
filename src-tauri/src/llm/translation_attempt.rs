// =============================================================================
// TRANSLATION_ATTEMPT.RS - One backend/tier attempt state machine
// =============================================================================
// The per-attempt loop of a single backend/tier translation: request,
// transient retry, validation invocation, typed outcome, and the diagnostics
// those steps write. It is deliberately knob-driven - no config, no context
// storage, no backend registry - so the higher-level owners (context-tier
// planner, backend fallback) in `manager.rs` pass every policy decision in.
// =============================================================================

use super::output_validation::{quality_issue_message, validate_translation_output};
use crate::llm::{
    LlmError, PromptRouterOptions, ReadyState, TranslationDiagnosticsState, TranslatorBackend,
};
use crate::sync_utils::lock_or_recover;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::time::{timeout, Duration};
use tracing::warn;

/// How one attempt sequence may run: retry count, delays, per-attempt caps,
/// and the prompt-router knobs.
#[derive(Clone)]
pub(super) struct AttemptPolicy {
    pub(super) max_attempts: usize,
    pub(super) retry_delay_ms: u64,
    pub(super) contexted_attempt_cap_ms: u64,
    pub(super) uncontexted_attempt_cap_ms: u64,
    pub(super) prompt_max_context_chars: usize,
    pub(super) prompt_max_source_chars: usize,
}

/// The deadline window an attempt sequence shares with its caller.
///
/// `started` is the caller's clock, not the runner's: the context-tier loop
/// creates it once per backend sequence, and the latency values measured
/// against it drive the tier planner's slow-degrade decision.
pub(super) struct AttemptBudget {
    pub(super) started: Instant,
    pub(super) total_timeout: Duration,
}

/// What one attempt sequence should translate.
pub(super) struct AttemptRequest<'a> {
    pub(super) text: &'a str,
    pub(super) source_language: &'a str,
    pub(super) target_language: &'a str,
    pub(super) context_prompt: Option<&'a str>,
    pub(super) context_used: bool,
}

/// Typed outcome of one attempt sequence.
#[derive(Debug)]
pub(super) enum AttemptOutcome {
    /// Translation succeeded: text, latency (shared clock), recovered_after_retry.
    Succeeded {
        translated: String,
        latency_ms: u128,
        recovered_after_retry: bool,
    },
    /// Timed out. `total_exhausted` tells the tier planner whether the whole
    /// budget is gone (stop degrading) or only the attempt was lost.
    TimedOut { total_exhausted: bool },
    /// Non-retryable error, or retries exhausted.
    Failed(LlmError),
}

/// Executes one backend/tier attempt sequence.
pub(super) struct TranslationAttemptRunner {
    diagnostics: Arc<Mutex<TranslationDiagnosticsState>>,
    policy: AttemptPolicy,
}

impl TranslationAttemptRunner {
    pub(super) fn new(
        policy: AttemptPolicy,
        diagnostics: Arc<Mutex<TranslationDiagnosticsState>>,
    ) -> Self {
        Self {
            diagnostics,
            policy,
        }
    }

    pub(super) async fn run(
        &self,
        backend: &dyn TranslatorBackend,
        request: &AttemptRequest<'_>,
        budget: &AttemptBudget,
        ready_state: ReadyState,
        warnings: &mut Vec<String>,
    ) -> AttemptOutcome {
        let id = backend.id();
        let text = request.text;
        let source_language = request.source_language;
        let target_language = request.target_language;
        let context_prompt = request.context_prompt;
        let context_used = request.context_used;
        let started = budget.started;
        let total_timeout = budget.total_timeout;
        let max_attempts = self.policy.max_attempts;

        for attempt in 1..=max_attempts {
            let remaining_total = total_timeout.saturating_sub(started.elapsed());
            if remaining_total.is_zero() {
                let latency_ms = started.elapsed().as_millis();
                lock_or_recover(&self.diagnostics).record_error(id, "timeout", Some(latency_ms));
                warn!(
                    backend_id = id.as_str(),
                    ready_state = ?ready_state,
                    latency_ms,
                    error_code = "timeout",
                    "Translation backend timed out"
                );
                warnings.push(format!("{}: timeout", id.as_str()));
                return AttemptOutcome::TimedOut {
                    total_exhausted: true,
                };
            }

            // Every attempt is bounded. With context the cap is tight, since a
            // slow answer has somewhere to go: drop a tier and ask again.
            // Without context there was no cap at all historically, so an
            // attempt ran to the full 30s total - and because the capture loop
            // awaits translation inline, one 27.6s stall left the pipeline
            // blind for its duration. The caps themselves are policy knobs.
            let attempt_cap = if context_used {
                self.policy.contexted_attempt_cap_ms
            } else {
                self.policy.uncontexted_attempt_cap_ms
            };
            let attempt_timeout = remaining_total.min(Duration::from_millis(attempt_cap));

            let result = timeout(
                attempt_timeout,
                backend.translate_with_context_options(
                    text,
                    source_language,
                    target_language,
                    context_prompt,
                    Some(PromptRouterOptions {
                        enable_context: context_used,
                        max_context_chars: self.policy.prompt_max_context_chars,
                        max_source_chars: self.policy.prompt_max_source_chars,
                    }),
                ),
            )
            .await;
            let latency_ms = started.elapsed().as_millis();

            match result {
                Ok(Ok(translated)) => {
                    if let Err(reason) = validate_translation_output(
                        text,
                        &translated,
                        source_language,
                        target_language,
                    ) {
                        lock_or_recover(&self.diagnostics).record_error(
                            id,
                            "low_quality_output",
                            Some(latency_ms),
                        );
                        warn!(
                            backend_id = id.as_str(),
                            ready_state = ?ready_state,
                            latency_ms,
                            error_code = "low_quality_output",
                            quality_issue = reason.code(),
                            attempt,
                            max_attempts,
                            "Translation output rejected"
                        );

                        return AttemptOutcome::Failed(LlmError::TranslationError(
                            quality_issue_message(reason),
                        ));
                    }

                    lock_or_recover(&self.diagnostics).record_success(id, latency_ms);
                    return AttemptOutcome::Succeeded {
                        translated,
                        latency_ms,
                        recovered_after_retry: attempt > 1,
                    };
                }
                Ok(Err(err)) => {
                    let should_retry =
                        attempt < max_attempts && crate::llm::transport_errors::is_transient(&err);
                    lock_or_recover(&self.diagnostics).record_error(
                        id,
                        err.code(),
                        Some(latency_ms),
                    );
                    warn!(
                        backend_id = id.as_str(),
                        ready_state = ?ready_state,
                        latency_ms,
                        error_code = err.code(),
                        attempt,
                        max_attempts,
                        "Translation backend failed: {}",
                        err
                    );

                    if should_retry {
                        let delay = Duration::from_millis(
                            self.policy.retry_delay_ms.saturating_mul(attempt as u64),
                        );
                        let remaining = total_timeout.saturating_sub(started.elapsed());
                        if remaining > delay {
                            tokio::time::sleep(delay).await;
                            continue;
                        }
                    }

                    return AttemptOutcome::Failed(err);
                }
                Err(_) => {
                    lock_or_recover(&self.diagnostics).record_error(
                        id,
                        "timeout",
                        Some(latency_ms),
                    );
                    warn!(
                        backend_id = id.as_str(),
                        ready_state = ?ready_state,
                        latency_ms,
                        error_code = "timeout",
                        attempt,
                        max_attempts,
                        "Translation backend timed out"
                    );

                    // With context, a timeout is answered by degrading a tier.
                    // Without context that door is shut, and abandoning the line
                    // is what put raw Chinese and an unavailable notice on
                    // screen. A stall clears on retry - the 27.6s one was
                    // followed by a 476ms answer - so ask again if budget allows.
                    if !context_used && attempt < max_attempts {
                        let remaining = total_timeout.saturating_sub(started.elapsed());
                        if remaining > Duration::from_millis(self.policy.retry_delay_ms) {
                            continue;
                        }
                    }

                    warnings.push(format!("{}: timeout", id.as_str()));
                    return AttemptOutcome::TimedOut {
                        total_exhausted: false,
                    };
                }
            }
        }

        // Should not reach here, but fallback
        AttemptOutcome::TimedOut {
            total_exhausted: true,
        }
    }
}

#[cfg(test)]
#[path = "translation_attempt_test_fixtures.rs"]
pub(crate) mod test_fixtures;

#[cfg(test)]
#[path = "translation_attempt_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "translation_attempt_timeout_tests.rs"]
mod timeout_tests;
