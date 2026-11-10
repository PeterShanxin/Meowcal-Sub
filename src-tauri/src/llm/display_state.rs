//! What the overlay shows when the engine's answer could not be used.

use super::TranslationDisplayState;

pub(crate) fn retryable_cue(state: TranslationDisplayState, warnings: &[String]) -> bool {
    match state {
        TranslationDisplayState::Warming | TranslationDisplayState::EngineSlow => true,
        // Unavailable also covers deterministic output rejection. Only retry a
        // held cue when its underlying failure can recover.
        TranslationDisplayState::TemporarilyUnavailable => warnings.iter().any(|warning| {
            let Some(reason) = warning.strip_prefix("local_engine: ") else {
                return false;
            };
            reason == "engine_recovery_failed"
                || reason.strip_prefix("API error: ").is_some_and(|message| {
                    super::transport_errors::is_transient(&super::LlmError::ApiError(
                        message.into(),
                    ))
                })
        }),
        _ => false,
    }
}

/// The display state after every engine attempt failed and a fallback backend
/// produced the text. Timeouts alone mean the engine is answering too slowly,
/// usually because the machine is busy, so the viewer is told to free the
/// CPU rather than that the engine is down.
pub(super) fn after_fallback(warnings: &[String]) -> TranslationDisplayState {
    let engine_warnings: Vec<String> = warnings
        .iter()
        .filter(|warning| warning.starts_with("local_engine:"))
        .map(|warning| warning.to_ascii_lowercase())
        .collect();

    if engine_warnings
        .iter()
        .any(|warning| warning.contains("not_ready") || warning.contains("engine_recovering"))
    {
        return TranslationDisplayState::Warming;
    }

    if !engine_warnings.is_empty()
        && engine_warnings
            .iter()
            .all(|warning| warning == "local_engine: timeout")
    {
        return TranslationDisplayState::EngineSlow;
    }

    if engine_warnings.iter().any(|warning| {
        !warning.contains("disabled")
            && !warning.contains("not_available")
            && !warning.contains("fallback used")
    }) {
        return TranslationDisplayState::TemporarilyUnavailable;
    }

    TranslationDisplayState::SourceOnly
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline_session::PipelineToken;
    use crate::subtitle_queue::{Completion, Frame, Presentation, SubtitleQueue};
    use std::time::{Duration, Instant};

    fn state(warnings: &[&str]) -> TranslationDisplayState {
        let warnings: Vec<String> = warnings.iter().map(|warning| warning.to_string()).collect();
        after_fallback(&warnings)
    }

    #[test]
    fn engine_timeouts_alone_say_the_engine_is_slow() {
        assert_eq!(
            state(&[
                "local_engine: timeout",
                "local_engine: timeout",
                "mock: disabled"
            ]),
            TranslationDisplayState::EngineSlow
        );
    }

    #[test]
    fn a_timeout_beside_another_failure_is_an_outage() {
        assert_eq!(
            state(&[
                "local_engine: timeout",
                "local_engine: engine_recovery_failed"
            ]),
            TranslationDisplayState::TemporarilyUnavailable
        );
    }

    #[test]
    fn a_starting_engine_is_warming_and_no_engine_is_source_only() {
        assert_eq!(
            state(&["local_engine: not_ready"]),
            TranslationDisplayState::Warming
        );
        assert_eq!(
            state(&["mock: disabled"]),
            TranslationDisplayState::SourceOnly
        );
    }

    #[test]
    fn a_held_cue_retries_after_transport_or_engine_recovery_then_stops_on_success() {
        for failure in [
            "local_engine: engine_recovery_failed",
            "local_engine: API error: CORE_STDOUT_EOF",
        ] {
            let warnings = vec![failure.to_string()];
            let state = after_fallback(&warnings);
            assert_eq!(state, TranslationDisplayState::TemporarilyUnavailable);
            let start = Instant::now();
            let frame = |ms| Frame {
                token: PipelineToken {
                    session_id: 1,
                    capture_id: ms,
                },
                region_generation: 0,
                text: "Wait here.".into(),
                context_prompt: None,
                started: start + Duration::from_millis(ms),
                capture_ms: 5,
                ocr_ms: 40,
                sample_ms: 125,
            };
            let mut queue = SubtitleQueue::new(Duration::from_millis(125));
            queue.observe(frame(0), true, start);
            let cue = queue
                .observe(frame(125), true, start + Duration::from_millis(125))
                .unwrap();
            let job = queue.next_job(start + Duration::from_millis(200)).unwrap();
            queue.complete(
                &job,
                Completion {
                    result: "unavailable",
                    text: String::new(),
                    presentation: Presentation::Preserve,
                    retryable: retryable_cue(state, &warnings),
                },
                start + Duration::from_millis(500),
            );
            queue.next_display(start + Duration::from_millis(500));
            assert!(queue
                .observe(frame(2999), false, start + Duration::from_millis(2999))
                .is_none());
            assert_eq!(
                queue.observe(frame(3000), false, start + Duration::from_millis(3000)),
                Some(cue),
                "{failure}"
            );
            let retry = queue.next_job(start + Duration::from_millis(3000)).unwrap();
            assert_eq!(retry.cue_id, job.cue_id);
            assert!(queue
                .observe(frame(3125), false, start + Duration::from_millis(3125))
                .is_none());
            queue.complete(
                &retry,
                Completion {
                    result: "success",
                    text: "Stay.".into(),
                    presentation: Presentation::Translated,
                    retryable: false,
                },
                start + Duration::from_millis(3500),
            );
            queue.next_display(start + Duration::from_millis(3500));
            for ms in [6000, 20_000] {
                assert!(queue
                    .observe(frame(ms), false, start + Duration::from_millis(ms))
                    .is_none());
            }
            assert_eq!(queue.stats(start).enqueued, 2);
            assert_eq!(queue.stats(start).depth, 0);
        }
    }

    #[test]
    fn deterministic_failures_do_not_retry_the_same_cue() {
        for error in [
            super::super::LlmError::TranslationError(
                "connection refused in rejected output".into(),
            ),
            super::super::LlmError::TranslationError("incorrect output language".into()),
            super::super::LlmError::ApiError("CORE_PROTOCOL_MISMATCH: unsupported API".into()),
            super::super::LlmError::ApiError("API error 404 Not Found".into()),
        ] {
            let warnings = vec![format!("local_engine: {error}")];
            assert!(
                !retryable_cue(after_fallback(&warnings), &warnings),
                "{error}"
            );
        }
    }
}
