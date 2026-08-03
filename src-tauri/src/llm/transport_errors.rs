// =============================================================================
// TRANSPORT_ERRORS.RS - Telling a broken connection from a broken model
// =============================================================================
// Two halves of one decision live here. `describe_request_failure` turns a
// failed HTTP call into the message the rest of the app carries, and
// `is_transient` reads that message back to decide whether asking again is
// worth a subtitle frame. They only agree if they are written together: the
// classifier can only match on words the description bothered to include.
//
// It did not. `reqwest::Error` renders as "error sending request for url (...)"
// and stops; whether the socket was refused, reset, or closed underneath a
// reused keep-alive connection sits one or more levels down the source chain.
// The description dropped it, the classifier looked for it and never found it,
// and so every transport failure was treated as permanent - which is how a
// local model that was running fine put "temporarily unavailable" on screen.
// =============================================================================

use crate::llm::LlmError;

/// Describe a failed request, including why it failed.
///
/// Walks the source chain so the reason survives into the message the retry
/// classifier reads.
pub(crate) fn describe_request_failure(error: &reqwest::Error) -> String {
    let mut message = format!("Request failed: {error}");
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        message.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    message
}

/// Whether an error is worth one more attempt inside a subtitle frame budget.
pub(crate) fn is_transient(err: &LlmError) -> bool {
    let LlmError::ApiError(message) = err else {
        return false;
    };
    let lower = message.to_ascii_lowercase();

    // A missing or unpermissioned ep_cache_context will not resolve within a
    // frame budget, so retrying only adds latency.
    if lower.contains("ep_cache_context")
        && (lower.contains("does not exist") || lower.contains("not accessible"))
    {
        return false;
    }

    // A pooled keep-alive socket the server has already closed fails on the way
    // out, before the model sees the request. Repeating it is safe and almost
    // always succeeds, so these are transport failures rather than refusals.
    //
    // Windows words these its own way: WSAECONNABORTED reads "An established
    // connection was aborted by the software in your host machine" and
    // WSAECONNRESET "An existing connection was forcibly closed by the remote
    // host", neither of which contains the phrases the Unix-shaped markers look
    // for. The os error numbers are matched too, since they survive whatever
    // wording a future Windows build ships.
    const TRANSIENT: [&str; 13] = [
        "model is loading",
        "connection refused",
        "connection reset",
        "connection was aborted",
        "forcibly closed",
        "os error 10053",
        "os error 10054",
        "connection closed before message completed",
        "broken pipe",
        "temporarily unavailable",
        "qnn_backend_manager",
        "onnxruntime::qnn",
        "failed to load from epcontext model",
    ];

    TRANSIENT.iter().any(|marker| lower.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api(message: &str) -> LlmError {
        LlmError::ApiError(message.to_string())
    }

    // The failure observed twice in a 45-minute session: a reused connection the
    // local server had already closed. It reached the classifier as reqwest's
    // bare Display and was judged permanent.
    #[test]
    fn a_dropped_keep_alive_connection_is_transient() {
        for message in [
            "Request failed: error sending request for url (http://127.0.0.1:11436/v1/chat/completions): connection closed before message completed",
            "Request failed: error sending request: connection reset by peer",
            "Request failed: error sending request: An established connection was aborted by the software in your host machine. (os error 10053)",
            "Request failed: error sending request: An existing connection was forcibly closed by the remote host. (os error 10054)",
            "Request failed: error sending request: broken pipe",
        ] {
            assert!(is_transient(&api(message)), "{message}");
        }
    }

    #[test]
    fn a_model_still_warming_is_transient() {
        assert!(is_transient(&api("API error: model is loading")));
        assert!(is_transient(&api(
            "failed to load from EPContext model, falling back"
        )));
    }

    // A description that stopped at reqwest's Display carries no reason, and
    // nothing downstream can invent one. This is the shape the fix removes.
    #[test]
    fn a_failure_with_no_stated_reason_is_not_assumed_transient() {
        assert!(!is_transient(&api(
            "Request failed: error sending request for url (http://127.0.0.1:11436/v1/chat/completions)"
        )));
    }

    #[test]
    fn a_missing_cache_context_is_not_worth_a_retry() {
        assert!(!is_transient(&api(
            "API error: ep_cache_context does not exist"
        )));
        assert!(!is_transient(&api(
            "API error: ep_cache_context is not accessible, connection reset"
        )));
    }

    #[test]
    fn a_rejected_translation_is_not_a_transport_failure() {
        assert!(!is_transient(&LlmError::TranslationError(
            "Translation output rejected as corrupted (incorrect output language).".to_string()
        )));
        assert!(!is_transient(&api("API error 404 Not Found")));
    }
}
