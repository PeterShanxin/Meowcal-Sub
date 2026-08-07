// =============================================================================
// CHAT_WIRE.RS - the OpenAI-compatible request and response shapes
// =============================================================================
// The engine speaks the OpenAI chat-completions dialect. These are the parts of
// it this app sends and reads, kept apart from the backend that drives them so
// the wire contract can be tested without a server and so `foundry_local` is
// about behaviour rather than serde attributes.
//
// Everything optional here is optional on purpose. An engine that omits a field
// must still produce a usable translation: losing a subtitle to a missing
// diagnostic would be absurd.
// =============================================================================

use serde::{Deserialize, Serialize};

/// Request to /v1/chat/completions endpoint
#[derive(Debug, Serialize)]
pub(super) struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: f32,
    pub top_k: u32,
    pub top_p: f32,
    pub repeat_penalty: f32,
    pub max_tokens: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Response from /v1/chat/completions endpoint
#[derive(Debug, Deserialize)]
pub(super) struct ChatCompletionResponse {
    pub choices: Vec<ChatChoice>,
    /// Absent on engines that do not report it, so never required.
    #[serde(default)]
    pub usage: Option<ChatUsage>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ChatChoice {
    pub message: ChatMessage,
}

/// Token accounting, used only for diagnostics.
///
/// Elapsed milliseconds alone cannot separate "the model wrote a lot" from
/// "the process was starved of CPU", and that distinction is the whole
/// difference between a slow line and issue #60. The engine reports the token
/// count; dividing it by the time we measured gives the generation rate that
/// does separate them.
#[derive(Debug, Deserialize)]
pub(super) struct ChatUsage {
    #[serde(default)]
    pub completion_tokens: u32,
}

impl ChatCompletionResponse {
    /// Completion tokens the engine reported, or zero when it reported none.
    pub(super) fn completion_tokens(&self) -> u32 {
        self.usage
            .as_ref()
            .map(|usage| usage.completion_tokens)
            .unwrap_or_default()
    }
}

/// Tokens per second for a completed request.
///
/// `None` when the engine reported no tokens or the request registered no
/// elapsed time - both mean the rate is unknown, and a fabricated zero or an
/// infinity in the log would be worse than an absent field.
pub(super) fn generation_rate(completion_tokens: u32, elapsed_ms: u64) -> Option<f64> {
    if completion_tokens == 0 || elapsed_ms == 0 {
        return None;
    }
    Some(f64::from(completion_tokens) * 1000.0 / elapsed_ms as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The distinction the field exists to make. A 20-second call that generated
    // 300 tokens is a long line; one that generated 20 is a starved process,
    // and elapsed milliseconds alone cannot tell them apart. See issue #60.
    #[test]
    fn generation_rate_separates_a_long_answer_from_a_starved_engine() {
        assert_eq!(generation_rate(300, 20_000), Some(15.0));
        assert_eq!(generation_rate(20, 20_000), Some(1.0));
    }

    #[test]
    fn generation_rate_is_absent_rather_than_wrong_when_it_cannot_be_known() {
        // An engine that reports no usage block, and a request that registered
        // no elapsed time. Logging 0.0 or an infinity would read as measurement.
        assert_eq!(generation_rate(0, 500), None);
        assert_eq!(generation_rate(12, 0), None);
    }

    // `usage` is optional on purpose: a response without it must still parse,
    // because losing the translation to a missing diagnostic would be absurd.
    #[test]
    fn a_response_without_usage_still_parses() {
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"你好"}}]}"#;
        let parsed: ChatCompletionResponse =
            serde_json::from_str(body).expect("a response without usage must still parse");

        assert!(parsed.usage.is_none());
        assert_eq!(parsed.completion_tokens(), 0);
        assert_eq!(
            parsed.choices.first().map(|c| c.message.content.as_str()),
            Some("你好")
        );
    }

    #[test]
    fn a_response_with_usage_carries_the_completion_token_count() {
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"你好"}}],
                       "usage":{"prompt_tokens":19,"completion_tokens":12}}"#;
        let parsed: ChatCompletionResponse =
            serde_json::from_str(body).expect("a response with usage should parse");

        assert_eq!(parsed.completion_tokens(), 12);
    }
}
