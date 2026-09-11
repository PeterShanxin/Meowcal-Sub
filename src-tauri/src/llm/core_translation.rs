use super::chat_wire::{ChatCompletionRequest, ChatCompletionResponse};
use super::{LlmError, ReadyState};

pub(super) fn is_available() -> bool {
    crate::core_client::status_blocking()
        .map(|status| status.installed)
        .unwrap_or(false)
}

pub(super) fn ready_state() -> ReadyState {
    match crate::core_client::status_blocking() {
        Ok(status) if status.ready => ReadyState::Ready,
        Ok(_) => ReadyState::NotReady,
        Err(_) => ReadyState::Error,
    }
}

pub(super) fn notes() -> String {
    match crate::core_client::status_blocking() {
        Ok(status) if status.ready => "Local Translation Engine is ready.".to_string(),
        Ok(status) if status.installed => {
            "Local Translation Engine is installed but stopped.".to_string()
        }
        Ok(_) => "Local Translation Engine is not installed.".to_string(),
        Err(error) => format!("Local Translation Engine unavailable: {error}"),
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
            if error.starts_with("CORE_NOT_READY:") {
                tokio::spawn(async {
                    let _ = crate::core_client::ready(std::time::Duration::from_secs(90)).await;
                });
            }
            return Err(LlmError::ApiError(error));
        }
    };
    serde_json::from_value(response)
        .map_err(|error| LlmError::ApiError(format!("Failed to parse response: {error}")))
}
