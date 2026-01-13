// =============================================================================
// LLM MODULE - Translation Backends and Manager
// =============================================================================
// This module defines:
// 1. Backend interfaces and readiness states
// 2. Backend selection + fallback manager
// 3. Implementations for each backend (Windows AI, Offline MT, Edge, Mock)
// =============================================================================

mod phi_silica;
mod manager;
mod offline_mt;
mod edge_translator;
mod mock;

pub use phi_silica::*;
pub use manager::*;
pub use offline_mt::*;
pub use edge_translator::*;
pub use mock::*;

use async_trait::async_trait;
use serde::Serialize;
use thiserror::Error;

/// Errors that can occur during translation
#[derive(Error, Debug)]
pub enum LlmError {
    #[error("Translation failed: {0}")]
    TranslationError(String),

    #[error("Model not available: {0}")]
    ModelNotAvailable(String),

    #[error("API error: {0}")]
    ApiError(String),
}

/// Known translation backend identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BackendId {
    WindowsAi,
    OfflineMt,
    EdgeTranslator,
    Mock,
}

impl BackendId {
    /// Canonical string id for logs and config
    pub fn as_str(&self) -> &'static str {
        match self {
            BackendId::WindowsAi => "windows_ai",
            BackendId::OfflineMt => "offline_mt",
            BackendId::EdgeTranslator => "edge_translator",
            BackendId::Mock => "mock",
        }
    }

    /// Parse a backend id from config strings
    pub fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "windows_ai" | "windowsai" | "windows-ai" | "phi" | "phi_silica" => Some(BackendId::WindowsAi),
            "offline_mt" | "offlinemt" | "offline-mt" | "translatelocally" => Some(BackendId::OfflineMt),
            "edge_translator" | "edgetranslator" | "edge-translator" => Some(BackendId::EdgeTranslator),
            "mock" | "passthrough" | "none" => Some(BackendId::Mock),
            _ => None,
        }
    }
}

/// Backend readiness states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReadyState {
    NotSupported,
    NotReady,
    Ready,
    Error,
}

/// Backend status for frontend/diagnostics
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendInfo {
    pub id: BackendId,
    pub name: String,
    pub available: bool,
    pub ready_state: ReadyState,
    pub notes: String,
}

/// Result returned by translation manager
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationOutcome {
    pub translated: String,
    pub backend_used: BackendId,
    pub warnings: Vec<String>,
}

/// Translation backend interface
#[async_trait]
pub trait TranslatorBackend: Send + Sync {
    /// Backend identifier
    fn id(&self) -> BackendId;

    /// Human-readable name
    fn name(&self) -> &'static str;

    /// Static availability check (binary present, APIs exposed, etc.)
    fn is_available(&self) -> bool;

    /// Current readiness state (model ready, API accessible, etc.)
    fn ready_state(&self) -> ReadyState;

    /// Short diagnostic note for UI/logs
    fn notes(&self) -> String;

    /// Translate text to target language
    async fn translate(
        &self,
        text: &str,
        source_language: &str,
        target_language: &str,
    ) -> Result<String, LlmError>;
}
