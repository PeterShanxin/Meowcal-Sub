// =============================================================================
// MOCK.RS - Passthrough Mock Backend
// =============================================================================

use crate::llm::{BackendId, LlmError, ReadyState, TranslatorBackend};
use async_trait::async_trait;

/// Passthrough backend (returns input as-is)
pub struct MockBackend;

impl MockBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockBackend {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl TranslatorBackend for MockBackend {
    fn id(&self) -> BackendId {
        BackendId::Mock
    }

    fn name(&self) -> &'static str {
        "Passthrough"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn ready_state(&self) -> ReadyState {
        ReadyState::Ready
    }

    fn notes(&self) -> String {
        "No translation (returns OCR text)".to_string()
    }

    async fn translate(
        &self,
        text: &str,
        _source_language: &str,
        _target_language: &str,
    ) -> Result<String, LlmError> {
        Ok(text.to_string())
    }
}
