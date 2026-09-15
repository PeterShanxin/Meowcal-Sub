use super::chat_wire::{ChatCompletionRequest, ChatCompletionResponse};
use super::{LlmError, ReadyState};

pub(super) fn is_available() -> bool {
    crate::core_client::cached_status()
        .map(|status| status.installed)
        .unwrap_or(false)
}

pub(super) fn ready_state() -> ReadyState {
    if crate::core_client::recovering() {
        return ReadyState::Recovering;
    }
    if crate::core_client::recovery_failed() {
        return ReadyState::RecoveryFailed;
    }
    match crate::core_client::cached_status() {
        Some(status) if status.ready => ReadyState::Ready,
        _ => ReadyState::NotReady,
    }
}

pub(super) fn notes() -> String {
    if crate::core_client::recovering() {
        return "Translation engine is recovering. Please wait.".into();
    }
    if crate::core_client::recovery_failed() {
        return "Engine recovery failed. Retry the engine in Settings.".into();
    }
    match crate::core_client::cached_status() {
        Some(status) if status.ready => "Local Translation Engine is ready.".to_string(),
        Some(status) if status.installed => {
            "Local Translation Engine is installed but stopped.".to_string()
        }
        Some(_) => "Local Translation Engine is not installed.".to_string(),
        None => "Local Translation Engine has not been checked.".to_string(),
    }
}

pub(super) async fn models() -> Result<Vec<String>, LlmError> {
    let status = crate::core_client::status()
        .await
        .map_err(LlmError::ApiError)?;
    Ok(status
        .installed
        .then_some(status.model)
        .into_iter()
        .collect())
}

pub(super) async fn is_ready() -> Result<bool, LlmError> {
    crate::core_client::status()
        .await
        .map(|status| status.ready)
        .map_err(LlmError::ApiError)
}

pub(super) async fn complete(
    request: &ChatCompletionRequest,
    timeout_ms: u64,
) -> Result<ChatCompletionResponse, LlmError> {
    let request = serde_json::to_value(request)
        .map_err(|error| LlmError::ApiError(format!("Invalid Core request: {error}")))?;
    let response = match crate::core_client::complete(request, timeout_ms).await {
        Ok(response) => response,
        Err(error) => {
            if super::transport_errors::is_transient(&LlmError::ApiError(error.clone())) {
                crate::core_client::recover_transport();
            }
            return Err(LlmError::ApiError(error));
        }
    };
    serde_json::from_value(response)
        .map_err(|error| LlmError::ApiError(format!("Failed to parse response: {error}")))
}

pub(super) async fn complete_translation(
    request: &ChatCompletionRequest,
    timeout_ms: u64,
    text: &str,
    source_language: &str,
    target_language: &str,
    max_source_chars: usize,
) -> Result<ChatCompletionResponse, LlmError> {
    let source = super::prompt_router::truncate_chars(
        &super::prompt_router::clean_source_text(text),
        max_source_chars,
    );
    let response = complete(request, timeout_ms).await?;
    let choice = response.choices.first();
    let output = choice
        .map(|choice| choice.message.content.trim())
        .unwrap_or_default();
    let result = validate_response(&source, output, source_language, target_language);
    tracing::info!(finish_reason = ?choice.and_then(|choice| choice.finish_reason.as_deref()),
        completion_tokens = response.completion_tokens(), max_tokens = request.max_tokens,
        source_chars = source.chars().count(), output_chars = output.chars().count(),
        receipt = ?response.inference_receipt, "Managed inference result");
    if let Err(reason) = result {
        tracing::warn!(
            error_code = "low_quality_output",
            quality_issue = reason.code(),
            "Managed inference rejected"
        );
        if let Some(receipt) = response.inference_receipt.as_ref() {
            crate::core_client::recover_inference(receipt.clone(), reason.code());
        }
        return Err(LlmError::TranslationError(
            super::output_validation::quality_issue_message(reason),
        ));
    }
    Ok(response)
}

fn validate_response(
    source: &str,
    output: &str,
    source_language: &str,
    target_language: &str,
) -> Result<(), super::output_validation::TranslationOutputRejection> {
    use super::output_validation::{validate_translation_output, TranslationOutputRejection};
    // Formatting can discard everything after a blank line. Preserve evidence
    // of corrupt generation before stripping headers or explanations.
    let raw = validate_translation_output(
        source,
        output,
        source_language,
        target_language,
        crate::translation_eligibility::Eligibility::AnyText,
    );
    if matches!(
        raw,
        Err(TranslationOutputRejection::TooLong
            | TranslationOutputRejection::RepetitionLoop
            | TranslationOutputRejection::GarbageTail
            | TranslationOutputRejection::PromptEcho)
    ) {
        return raw;
    }
    let sanitized = super::subtitle_output::sanitize_subtitle_translation_output(output);
    validate_translation_output(
        source,
        &sanitized,
        source_language,
        target_language,
        crate::translation_eligibility::Eligibility::AnyText,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_debris_before_sanitization_can_hide_it() {
        assert!(validate_response("The ferry leaves.", "渡轮离开。\n\n---+", "en", "zh").is_err());
        assert!(validate_response("Hello", "你好你好你好你好你好你好", "en", "zh").is_err());
        assert!(validate_response(
            "Hello",
            &format!("你好。\n\n{}", "garbage ".repeat(30)),
            "en",
            "zh"
        )
        .is_err());
        assert!(validate_response("Saber", "Translation: Saber", "en", "zh").is_ok());
        assert!(validate_response(
            "你好",
            "Translation: Hello.\n\nYou are a subtitle translator",
            "zh",
            "en"
        )
        .is_err());
    }
    #[test]
    fn normal_length_limited_general_text_remains_usable() {
        let output: String = (1..=30)
            .map(|index| format!("这是第{index}段屏幕文字。"))
            .collect();
        let response: ChatCompletionResponse = serde_json::from_value(serde_json::json!({
            "choices":[{"message":{"role":"assistant","content":output},"finish_reason":"length"}],
            "usage":{"completion_tokens":120}, "inferenceReceipt":"test"
        }))
        .unwrap();
        let choice = &response.choices[0];
        assert_eq!(choice.finish_reason.as_deref(), Some("length"));
        assert!(validate_response(
            &"This is ordinary screen text. ".repeat(30),
            &choice.message.content,
            "en",
            "zh"
        )
        .is_ok());
    }
}
