use super::chat_wire::{ChatCompletionRequest, ChatCompletionResponse};
use super::{LlmError, ReadyState};

pub(super) fn is_available() -> bool {
    crate::core_client::cached_status()
        .map(|status| status.installed)
        .unwrap_or(false)
}

pub(super) fn ready_state() -> ReadyState {
    match crate::core_client::cached_status() {
        Some(status) if status.ready => ReadyState::Ready,
        _ => ReadyState::NotReady,
    }
}

pub(super) fn notes() -> String {
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
                tokio::spawn(async {
                    let _ = crate::core_client::ready(crate::core_client::READY_TIMEOUT).await;
                });
            }
            return Err(LlmError::ApiError(error));
        }
    };
    serde_json::from_value(response)
        .map_err(|error| LlmError::ApiError(format!("Failed to parse response: {error}")))
}
