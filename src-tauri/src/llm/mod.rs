// =============================================================================
// LLM MODULE - Translation Backends and Manager
// =============================================================================
// This module defines:
// 1. Backend interfaces and readiness states
// 2. Backend selection + fallback manager
// 3. Implementations for each backend (Windows AI, Offline MT, Mock)
// =============================================================================

mod context;
mod foundry_local;
mod manager;
mod mock;
mod offline_mt;
mod phi_silica;
mod prompt_router;

pub use context::*;
pub use foundry_local::*;
pub use manager::*;
pub use mock::*;
pub use offline_mt::*;
pub use phi_silica::*;
pub use prompt_router::*;

use async_trait::async_trait;
use serde::Serialize;
use std::borrow::Cow;
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during translation
#[derive(Error, Debug, Clone)]
pub enum LlmError {
    #[error("Translation failed: {0}")]
    TranslationError(String),

    #[error("Model not available: {0}")]
    ModelNotAvailable(String),

    #[error("API error: {0}")]
    ApiError(String),
}

impl LlmError {
    /// Short, non-PII error code for diagnostics/logging.
    pub fn code(&self) -> &'static str {
        match self {
            LlmError::TranslationError(_) => "translation_error",
            LlmError::ModelNotAvailable(_) => "model_not_available",
            LlmError::ApiError(_) => "api_error",
        }
    }
}

/// Known translation backend identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BackendId {
    FoundryLocal,
    WindowsAi,
    OfflineMt,
    Mock,
}

impl BackendId {
    /// Canonical string id for logs and config
    pub fn as_str(&self) -> &'static str {
        match self {
            BackendId::FoundryLocal => "foundry_local",
            BackendId::WindowsAi => "windows_ai",
            BackendId::OfflineMt => "offline_mt",
            BackendId::Mock => "mock",
        }
    }

    /// Parse a backend id from config strings
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "foundry_local" | "foundrylocal" | "foundry-local" | "foundry" => {
                Some(BackendId::FoundryLocal)
            }
            "windows_ai" | "windowsai" | "windows-ai" | "phi" | "phi_silica" => {
                Some(BackendId::WindowsAi)
            }
            "offline_mt" | "offlinemt" | "offline-mt" | "translatelocally" => {
                Some(BackendId::OfflineMt)
            }
            "mock" | "passthrough" | "none" => Some(BackendId::Mock),
            _ => None,
        }
    }
}

/// Provide a human-friendly language label for LLM prompts.
pub fn language_prompt_label(code: &str) -> Cow<'_, str> {
    match code.trim() {
        "en-US" => Cow::Borrowed("English (US)"),
        "en-GB" => Cow::Borrowed("English (UK)"),
        "zh-CN" => Cow::Borrowed("Chinese (Simplified)"),
        "zh-TW" => Cow::Borrowed("Chinese (Traditional)"),
        "ja-JP" => Cow::Borrowed("Japanese"),
        "ko-KR" => Cow::Borrowed("Korean"),
        "es-ES" => Cow::Borrowed("Spanish"),
        "fr-FR" => Cow::Borrowed("French"),
        "de-DE" => Cow::Borrowed("German"),
        _ => Cow::Owned(code.trim().to_string()),
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

/// Foundry Local-specific status phases (more granular than generic ReadyState).
///
/// These phases provide detailed insight into Foundry Local's operational state,
/// distinguishing between installation issues, service state, model availability,
/// and warmup status (especially important for NPU models).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FoundryLocalPhase {
    /// Foundry CLI not found on the system
    NotInstalled,
    /// Foundry CLI exists but service is not running
    NotRunning,
    /// Service running but no models are cached
    NoModels,
    /// Service running with models, but no recent "ready to talk" probe has succeeded yet.
    ///
    /// This is distinct from `Preparing` (which means we *did* probe but it timed out).
    Unchecked,
    /// Service up and model known, but chat probe times out (warmup in progress)
    Preparing,
    /// Chat probe succeeds - ready for translation
    Ready,
    /// An error occurred during status check
    Error,
}

impl FoundryLocalPhase {
    /// Convert to the generic ReadyState for backward compatibility
    pub fn to_ready_state(&self) -> ReadyState {
        match self {
            FoundryLocalPhase::Ready => ReadyState::Ready,
            FoundryLocalPhase::NotInstalled => ReadyState::NotSupported,
            FoundryLocalPhase::NotRunning
            | FoundryLocalPhase::NoModels
            | FoundryLocalPhase::Unchecked
            | FoundryLocalPhase::Preparing => ReadyState::NotReady,
            FoundryLocalPhase::Error => ReadyState::Error,
        }
    }

    /// Get a human-readable label for UI display
    pub fn label(&self) -> &'static str {
        match self {
            FoundryLocalPhase::NotInstalled => "Not Installed",
            FoundryLocalPhase::NotRunning => "Not Running",
            FoundryLocalPhase::NoModels => "No Models",
            FoundryLocalPhase::Unchecked => "Not Checked",
            FoundryLocalPhase::Preparing => "Preparing",
            FoundryLocalPhase::Ready => "Ready",
            FoundryLocalPhase::Error => "Error",
        }
    }
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
    /// Optional granular phase (currently only used by Foundry Local)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<FoundryLocalPhase>,
}

/// Result returned by translation manager
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationOutcome {
    pub translated: String,
    pub backend_used: BackendId,
    pub warnings: Vec<String>,
}

/// Diagnostics snapshot returned to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationDiagnostics {
    pub backends: Vec<BackendInfo>,
    pub last_error_by_backend: HashMap<String, String>,
    pub last_latency_by_backend: HashMap<String, u128>,
}

/// Windows AI diagnostics snapshot for UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowsAiDiagnostics {
    pub ready_state: ReadyState,
    pub notes: String,
    pub runtime_class_present: bool,
    pub bindings_enabled: bool,
    pub packaged: bool,
    pub package_full_name: Option<String>,
    pub packaging_note: String,
    pub capability_note: String,
}

/// Internal diagnostics state (stored in AppState).
#[derive(Debug, Default)]
pub struct TranslationDiagnosticsState {
    last_error_by_backend: HashMap<String, String>,
    last_latency_by_backend: HashMap<String, u128>,
}

impl TranslationDiagnosticsState {
    pub fn record_success(&mut self, backend_id: BackendId, latency_ms: u128) {
        let key = backend_id.as_str().to_string();
        self.last_latency_by_backend.insert(key.clone(), latency_ms);
        self.last_error_by_backend.remove(&key);
    }

    pub fn record_error(
        &mut self,
        backend_id: BackendId,
        error_code: &str,
        latency_ms: Option<u128>,
    ) {
        let key = backend_id.as_str().to_string();
        if !error_code.is_empty() {
            self.last_error_by_backend
                .insert(key.clone(), error_code.to_string());
        }
        if let Some(latency) = latency_ms {
            self.last_latency_by_backend.insert(key, latency);
        }
    }

    pub fn snapshot(&self) -> (HashMap<String, String>, HashMap<String, u128>) {
        (
            self.last_error_by_backend.clone(),
            self.last_latency_by_backend.clone(),
        )
    }
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

    /// Translate text to target language, optionally providing session context.
    ///
    /// Default implementation ignores `context` and calls `translate()`.
    async fn translate_with_context(
        &self,
        text: &str,
        source_language: &str,
        target_language: &str,
        context: Option<&str>,
    ) -> Result<String, LlmError> {
        let _ = context;
        self.translate(text, source_language, target_language).await
    }

    /// Translate text with context plus prompt-building options (used by subtitle prompt router).
    ///
    /// Default implementation ignores `options` and delegates to `translate_with_context()`.
    async fn translate_with_context_options(
        &self,
        text: &str,
        source_language: &str,
        target_language: &str,
        context: Option<&str>,
        options: Option<PromptRouterOptions>,
    ) -> Result<String, LlmError> {
        let _ = options;
        self.translate_with_context(text, source_language, target_language, context)
            .await
    }
}
