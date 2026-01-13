// =============================================================================
// PHI_SILICA.RS - Windows Copilot Runtime / Phi Silica Integration
// =============================================================================
//
// Status: Stub with readiness probing.
// We can detect the runtime class, but the WinAppSDK bindings are not wired yet.
// This keeps the app responsive and lets the backend manager fall back cleanly.
// =============================================================================

use super::{BackendId, LlmError, ReadyState, TranslatorBackend};
use async_trait::async_trait;
use std::time::Duration;
use tracing::{debug, info, warn};

#[cfg(target_os = "windows")]
use windows::core::HSTRING;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{REGDB_E_CLASSNOTREG, RPC_E_CHANGED_MODE};
#[cfg(target_os = "windows")]
use windows::Win32::System::WinRT::{
    IActivationFactory, RoGetActivationFactory, RoInitialize, RO_INIT_MULTITHREADED,
};

/// WinRT runtime class name for the LanguageModel API.
const LANGUAGE_MODEL_RUNTIME_CLASS: &str = "Microsoft.Windows.AI.Text.LanguageModel";

/// Subtitles are short; keep a tight guard to avoid slow prompts.
const MAX_INPUT_CHARS: usize = 800;

/// Time budget for EnsureReadyAsync (when bindings are available).
const READY_TIMEOUT_MS: u64 = 1500;

/// PhiSilica - Windows Copilot Runtime LLM
///
/// Uses NPU-accelerated Phi Silica when available (future).
pub struct PhiSilica {
    runtime_class_present: bool,
    init_error: Option<String>,
    bindings_enabled: bool,
}

impl PhiSilica {
    /// Create a new Phi Silica backend instance.
    pub fn new() -> Self {
        let bindings_enabled = cfg!(all(target_os = "windows", feature = "windows_ai"));
        let (runtime_class_present, init_error) = detect_runtime_class();

        if runtime_class_present {
            info!("Windows AI runtime class detected.");
        } else {
            debug!("Windows AI runtime class not detected.");
        }

        if !bindings_enabled {
            debug!("Windows AI feature flag is disabled.");
        }

        if let Some(err) = &init_error {
            warn!("Windows AI probe error: {}", err);
        }

        Self {
            runtime_class_present,
            init_error,
            bindings_enabled,
        }
    }

    /// Determine readiness state and a diagnostic message.
    fn readiness(&self) -> (ReadyState, String) {
        if !cfg!(target_os = "windows") {
            return (
                ReadyState::NotSupported,
                "Windows-only backend.".to_string(),
            );
        }

        if let Some(err) = &self.init_error {
            return (
                ReadyState::Error,
                format!("WinRT initialization failed: {}", err),
            );
        }

        if !self.runtime_class_present {
            return (
                ReadyState::NotSupported,
                "LanguageModel runtime class not registered.".to_string(),
            );
        }

        if !self.bindings_enabled {
            return (
                ReadyState::NotSupported,
                "Feature `windows_ai` is disabled. Enable it and add WinAppSDK bindings."
                    .to_string(),
            );
        }

        // Bindings exist only when we generate and wire WinAppSDK metadata.
        (
            ReadyState::NotReady,
            "Windows AI runtime detected; bindings not wired yet.".to_string(),
        )
    }

    /// Public readiness check (placeholder).
    pub fn get_ready_state(&self) -> ReadyState {
        self.readiness().0
    }

    /// Ensure the model is ready (placeholder for EnsureReadyAsync).
    pub async fn ensure_ready(&self, timeout_ms: u64) -> Result<ReadyState, LlmError> {
        let (state, _) = self.readiness();
        if state != ReadyState::NotReady {
            return Ok(state);
        }

        // Placeholder: real EnsureReadyAsync will live behind windows_ai bindings.
        let timeout = Duration::from_millis(timeout_ms);
        let result = tokio::time::timeout(timeout, async { Ok(state) }).await;
        match result {
            Ok(state) => state,
            Err(_) => Err(LlmError::ApiError(format!(
                "Windows AI ensure_ready timed out after {}ms",
                timeout_ms
            ))),
        }
    }

    fn build_prompt(&self, text: &str, source_language: &str, target_language: &str) -> String {
        format!(
            "Translate from {} to {}. Output only the translated text:\n{}",
            source_language, target_language, text
        )
    }
}

#[async_trait]
impl TranslatorBackend for PhiSilica {
    fn id(&self) -> BackendId {
        BackendId::WindowsAi
    }

    fn name(&self) -> &'static str {
        "Windows AI (Phi Silica)"
    }

    fn is_available(&self) -> bool {
        let (state, _) = self.readiness();
        matches!(state, ReadyState::NotReady | ReadyState::Ready)
    }

    fn ready_state(&self) -> ReadyState {
        self.readiness().0
    }

    fn notes(&self) -> String {
        self.readiness().1
    }

    async fn translate(
        &self,
        text: &str,
        source_language: &str,
        target_language: &str,
    ) -> Result<String, LlmError> {
        if text.trim().is_empty() {
            return Ok(String::new());
        }

        if text.chars().count() > MAX_INPUT_CHARS {
            return Err(LlmError::TranslationError(format!(
                "Input too long for Windows AI (max {} chars)",
                MAX_INPUT_CHARS
            )));
        }

        let state = self.ensure_ready(READY_TIMEOUT_MS).await?;
        if state != ReadyState::Ready {
            let note = self.readiness().1;
            return Err(LlmError::ModelNotAvailable(format!(
                "Windows AI not ready: {}",
                note
            )));
        }

        let _prompt = self.build_prompt(text, source_language, target_language);

        Err(LlmError::ApiError(
            "Windows AI bindings not implemented yet".to_string(),
        ))
    }
}

fn detect_runtime_class() -> (bool, Option<String>) {
    #[cfg(target_os = "windows")]
    {
        // Initialize WinRT on this thread (ignore changed mode error).
        if let Err(err) = unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
            if err.code() != RPC_E_CHANGED_MODE {
                return (false, Some(format!("RoInitialize failed: {}", err)));
            }
        }

        let class_name = HSTRING::from(LANGUAGE_MODEL_RUNTIME_CLASS);
        match unsafe { RoGetActivationFactory::<IActivationFactory>(&class_name) } {
            Ok(_) => (true, None),
            Err(err) => {
                if err.code() == REGDB_E_CLASSNOTREG {
                    (false, None)
                } else {
                    (false, Some(format!("RoGetActivationFactory failed: {}", err)))
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        (false, None)
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_phi_silica() {
        let phi = PhiSilica::new();
        println!("Phi Silica ready state: {:?}", phi.ready_state());
    }
    
    #[tokio::test]
    async fn test_stub_translation() {
        let phi = PhiSilica::new();
        
        let result = phi.translate("Hello, world!", "en-US", "zh-CN").await;
        assert!(result.is_err());
    }
    
    #[tokio::test] 
    async fn test_empty_translation() {
        let phi = PhiSilica::new();
        
        let result = phi.translate("", "en-US", "zh-CN").await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
