//! What the overlay shows when the engine's answer could not be used.

use super::TranslationDisplayState;

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
}
