// =============================================================================
// EDGE_TRANSLATOR.RS - Experimental Edge Translator Backend (Stub for 04.0)
// =============================================================================

use crate::llm::{BackendId, LlmError, ReadyState, TranslatorBackend};
use async_trait::async_trait;
use tauri::AppHandle;

/// Experimental Edge Translator backend (WebView2)
pub struct EdgeTranslatorBackend {
    _app: AppHandle,
}

impl EdgeTranslatorBackend {
    pub fn new(app: AppHandle) -> Self {
        Self { _app: app }
    }
}

#[async_trait]
impl TranslatorBackend for EdgeTranslatorBackend {
    fn id(&self) -> BackendId {
        BackendId::EdgeTranslator
    }

    fn name(&self) -> &'static str {
        "Edge Translator (Experimental)"
    }

    fn is_available(&self) -> bool {
        false
    }

    fn ready_state(&self) -> ReadyState {
        ReadyState::NotSupported
    }

    fn notes(&self) -> String {
        "WebView2 Translator API not implemented".to_string()
    }

    async fn translate(
        &self,
        _text: &str,
        _source_language: &str,
        _target_language: &str,
    ) -> Result<String, LlmError> {
        Err(LlmError::ModelNotAvailable(
            "Edge Translator backend not implemented".to_string(),
        ))
    }
}
