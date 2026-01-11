// =============================================================================
// LLM MODULE - Language Model for Translation
// =============================================================================
// This module handles translation using AI language models.
// 
// Current status: PLACEHOLDER
// The Phi Silica API (Windows Copilot Runtime) is still in preview.
// We'll implement this when the APIs become stable.
// 
// For now, we return the original text as a "mock" translation.
// =============================================================================

mod phi_silica;

pub use phi_silica::*;

use async_trait::async_trait;
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

/// Trait for translation providers
/// 
/// This allows us to swap between different translation backends:
/// - Phi Silica (Windows AI API) - when available
/// - Ollama (local server) - fallback
/// - Cloud APIs - future option
#[async_trait]
pub trait TranslationProvider: Send + Sync {
    /// Translate text to the target language
    async fn translate(&self, text: &str, target_language: &str) -> Result<String, LlmError>;
    
    /// Get the name of this provider (for logging)
    fn name(&self) -> &'static str;
    
    /// Check if this provider is available
    fn is_available(&self) -> bool;
}

/// The result of a translation
#[derive(Debug, Clone)]
pub struct TranslationResult {
    /// The original text
    pub original: String,
    /// The translated text
    pub translated: String,
    /// The target language
    pub target_language: String,
    /// Which provider was used
    pub provider: String,
}
