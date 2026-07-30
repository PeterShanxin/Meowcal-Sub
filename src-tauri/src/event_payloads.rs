use crate::llm::TranslationDisplayState;
use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TranslationPayload {
    pub session_id: u64,
    pub capture_id: u64,
    pub original: String,
    pub translated: String,
    pub backend_used: String,
    pub warnings: Vec<String>,
    pub display_state: TranslationDisplayState,
    pub timestamp: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CaptureStatusPayload {
    pub using_fallback: bool,
    pub message: String,
    pub is_error: bool,
}
