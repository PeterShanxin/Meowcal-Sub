// =============================================================================
// MANAGER.RS - Translation Backend Selection + Fallback
// =============================================================================

use crate::config::TranslationConfig;
use crate::llm::{
    BackendId, BackendInfo, FoundryLocalBackend, MockBackend, OfflineMtBackend, PhiSilica,
    ReadyState, TranslationContext, TranslationDiagnostics, TranslationDiagnosticsState,
    TranslationOutcome, TranslatorBackend,
};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;
use tauri::AppHandle;
use tokio::time::{timeout, Duration};
use tracing::{debug, info, warn};

const DEFAULT_BACKEND_TIMEOUT_MS: u64 = 2500;
const MAX_TRANSLATION_INPUT_CHARS: usize = 2000;

/// Manages available translation backends and fallback selection
pub struct TranslationManager {
    config: TranslationConfig,
    backends: Vec<Box<dyn TranslatorBackend>>,
    diagnostics: Arc<Mutex<TranslationDiagnosticsState>>,
    backend_timeout_ms: u64,
    /// Session translation context (shared across calls)
    context: Arc<RwLock<TranslationContext>>,
}

impl TranslationManager {
    /// Create a new manager with the configured backends
    pub fn new(
        config: TranslationConfig,
        app: AppHandle,
        diagnostics: Arc<Mutex<TranslationDiagnosticsState>>,
    ) -> Self {
        let mut backends: Vec<Box<dyn TranslatorBackend>> = Vec::new();

        // Fallback order: Foundry Local -> Offline MT -> Windows AI -> Mock
        backends.push(Box::new(FoundryLocalBackend::new(config.foundry_local.clone())));
        backends.push(Box::new(OfflineMtBackend::new(app.clone(), config.offline_mt.clone())));
        backends.push(Box::new(PhiSilica::new()));
        backends.push(Box::new(MockBackend::new()));

        // Initialize context with auto-detected budget
        let context_budget = Self::detect_context_budget(&config);
        let context = TranslationContext::new(context_budget, config.enable_context_aware);

        Self {
            config,
            backends,
            diagnostics,
            backend_timeout_ms: DEFAULT_BACKEND_TIMEOUT_MS,
            context: Arc::new(RwLock::new(context)),
        }
    }

    /// Detect appropriate context budget based on model
    fn detect_context_budget(config: &TranslationConfig) -> usize {
        // Try to detect from Foundry Local model if available
        if config.enable_foundry_local {
            if let Some(ref model) = config.foundry_local.model {
                if let Some(window) = FoundryLocalBackend::get_model_context_window(model) {
                    // Use 15% of context window for translation context
                    let budget = (window as f32 * 0.15) as usize;
                    debug!("Context budget from model {}: {} tokens", model, budget);
                    return budget.clamp(200, 2000);
                }
            }

            // Try first cached model
            let cached_models = FoundryLocalBackend::get_cached_models_from_cli();
            if let Some(first_model) = cached_models.first() {
                if let Some(window) = FoundryLocalBackend::get_model_context_window(first_model) {
                    let budget = (window as f32 * 0.15) as usize;
                    debug!("Context budget from first cached model {}: {} tokens", first_model, budget);
                    return budget.clamp(200, 2000);
                }
            }
        }

        // Default budget if detection fails
        debug!("Using default context budget: 500 tokens");
        500
    }

    #[cfg(test)]
    pub fn with_backends(
        config: TranslationConfig,
        backends: Vec<Box<dyn TranslatorBackend>>,
        diagnostics: Arc<Mutex<TranslationDiagnosticsState>>,
        backend_timeout_ms: u64,
    ) -> Self {
        let context = TranslationContext::new(500, config.enable_context_aware);
        Self {
            config,
            backends,
            diagnostics,
            backend_timeout_ms,
            context: Arc::new(RwLock::new(context)),
        }
    }

    /// List backend status for UI/diagnostics
    pub fn list_backends(&self) -> Vec<BackendInfo> {
        self.backends
            .iter()
            .map(|backend| {
                let enabled = self.is_enabled(backend.id());
                let ready_state = if enabled {
                    backend.ready_state()
                } else {
                    ReadyState::NotSupported
                };
                let available = enabled && backend.is_available();
                let mut notes = backend.notes();

                if !enabled {
                    notes = if notes.is_empty() {
                        "Disabled by config".to_string()
                    } else {
                        format!("Disabled by config. {}", notes)
                    };
                }

                BackendInfo {
                    id: backend.id(),
                    name: backend.name().to_string(),
                    available,
                    ready_state,
                    notes,
                }
            })
            .collect()
    }

    /// Return diagnostics snapshot for frontend.
    pub fn diagnostics_snapshot(&self) -> TranslationDiagnostics {
        let backends = self.list_backends();
        let (last_error_by_backend, last_latency_by_backend) =
            self.diagnostics.lock().unwrap().snapshot();

        TranslationDiagnostics {
            backends,
            last_error_by_backend,
            last_latency_by_backend,
        }
    }

    /// Translate with fallback chain
    pub async fn translate_with_fallback(
        &self,
        text: &str,
        source_language: &str,
        target_language: &str,
    ) -> TranslationOutcome {
        if text.trim().is_empty() {
            return TranslationOutcome {
                translated: String::new(),
                backend_used: BackendId::Mock,
                warnings: Vec::new(),
            };
        }

        let input_chars = text.chars().count();
        if input_chars > MAX_TRANSLATION_INPUT_CHARS {
            let warning = format!(
                "input_too_long: max {} chars",
                MAX_TRANSLATION_INPUT_CHARS
            );
            self.diagnostics.lock().unwrap().record_error(
                BackendId::Mock,
                "input_too_long",
                None,
            );
            return TranslationOutcome {
                translated: text.to_string(),
                backend_used: BackendId::Mock,
                warnings: vec![warning],
            };
        }

        let mut warnings = Vec::new();
        let mut order = self.ordered_backend_ids();

        // Remove duplicates while preserving order
        let mut seen = Vec::new();
        order.retain(|id| {
            if seen.contains(id) {
                false
            } else {
                seen.push(*id);
                true
            }
        });

        for id in order {
            let backend = match self.backend_by_id(id) {
                Some(b) => b,
                None => {
                    warnings.push(format!("{}: backend_not_registered", id.as_str()));
                    self.diagnostics.lock().unwrap().record_error(
                        id,
                        "backend_not_registered",
                        None,
                    );
                    continue;
                }
            };

            if !self.is_enabled(id) {
                warnings.push(format!("{}: disabled", id.as_str()));
                self.diagnostics
                    .lock()
                    .unwrap()
                    .record_error(id, "disabled", None);
                debug!(
                    backend_id = id.as_str(),
                    ready_state = ?backend.ready_state(),
                    error_code = "disabled",
                    "Translation backend skipped"
                );
                continue;
            }

            if !backend.is_available() {
                warnings.push(format!("{}: not_available", id.as_str()));
                self.diagnostics
                    .lock()
                    .unwrap()
                    .record_error(id, "not_available", None);
                debug!(
                    backend_id = id.as_str(),
                    ready_state = ?backend.ready_state(),
                    error_code = "not_available",
                    "Translation backend skipped"
                );
                continue;
            }

            let ready_state = backend.ready_state();
            if ready_state != ReadyState::Ready {
                warnings.push(format!("{}: not_ready", id.as_str()));
                self.diagnostics
                    .lock()
                    .unwrap()
                    .record_error(id, "not_ready", None);
                debug!(
                    backend_id = id.as_str(),
                    ready_state = ?ready_state,
                    error_code = "not_ready",
                    "Translation backend skipped"
                );
                continue;
            }

            let started = Instant::now();
            let result = timeout(
                Duration::from_millis(self.backend_timeout_ms),
                backend.translate(text, source_language, target_language),
            )
            .await;
            let latency_ms = started.elapsed().as_millis();

            match result {
                Ok(Ok(translated)) => {
                    self.diagnostics
                        .lock()
                        .unwrap()
                        .record_success(id, latency_ms);
                    info!(
                        backend_id = id.as_str(),
                        ready_state = ?ready_state,
                        latency_ms,
                        error_code = "",
                        "Translation backend used"
                    );
                    return TranslationOutcome {
                        translated,
                        backend_used: id,
                        warnings,
                    };
                }
                Ok(Err(err)) => {
                    self.diagnostics
                        .lock()
                        .unwrap()
                        .record_error(id, err.code(), Some(latency_ms));
                    warn!(
                        backend_id = id.as_str(),
                        ready_state = ?ready_state,
                        latency_ms,
                        error_code = err.code(),
                        "Translation backend failed: {}",
                        err
                    );
                    warnings.push(format!("{}: {}", id.as_str(), err));
                }
                Err(_) => {
                    let error_code = "timeout";
                    self.diagnostics
                        .lock()
                        .unwrap()
                        .record_error(id, error_code, Some(latency_ms));
                    warn!(
                        backend_id = id.as_str(),
                        ready_state = ?ready_state,
                        latency_ms,
                        error_code,
                        "Translation backend timed out"
                    );
                    warnings.push(format!("{}: timeout", id.as_str()));
                }
            }
        }

        if self.config.allow_mock_fallback {
            if let Some(mock) = self.backend_by_id(BackendId::Mock) {
                match mock.translate(text, source_language, target_language).await {
                    Ok(translated) => {
                        self.diagnostics.lock().unwrap().record_success(
                            BackendId::Mock,
                            0,
                        );
                        warnings.push("mock: fallback used".to_string());
                        return TranslationOutcome {
                            translated,
                            backend_used: BackendId::Mock,
                            warnings,
                        };
                    }
                    Err(err) => {
                        self.diagnostics
                            .lock()
                            .unwrap()
                            .record_error(BackendId::Mock, err.code(), None);
                        warnings.push(format!("mock: {}", err));
                    }
                }
            }
        }

        // Last resort: passthrough to keep the app responsive
        warnings.push("no translation backend available".to_string());
        self.diagnostics
            .lock()
            .unwrap()
            .record_error(BackendId::Mock, "no_backend_available", None);
        TranslationOutcome {
            translated: text.to_string(),
            backend_used: BackendId::Mock,
            warnings,
        }
    }

    /// Check if text is duplicate (for deduplication in capture loop)
    pub fn is_duplicate(&self, text: &str) -> bool {
        if !self.config.enable_context_aware {
            return false;
        }
        self.context.read().unwrap().is_duplicate(text)
    }

    /// Record a successful translation in context
    pub fn record_translation(&self, source: &str, translation: &str) {
        if !self.config.enable_context_aware {
            return;
        }
        self.context.write().unwrap().add_translation(source, translation);
    }

    /// Get context prompt to enhance translation request
    pub fn get_context_prompt(&self) -> Option<String> {
        if !self.config.enable_context_aware {
            return None;
        }
        self.context.read().unwrap().build_context_prompt()
    }

    /// Check if context needs compression (memory summarization)
    pub fn needs_context_compression(&self) -> bool {
        self.context.read().unwrap().needs_compression()
    }

    /// Get history entries for summarization
    pub fn get_history_for_summarization(&self) -> Vec<crate::llm::HistoryEntry> {
        self.context.write().unwrap().get_history_for_summarization()
    }

    /// Update context memory with summarized content
    pub fn update_context_memory(&self, memory: String) {
        self.context.write().unwrap().set_memory(memory);
    }

    /// Reset context (call when capture session ends)
    pub fn reset_context(&self) {
        self.context.write().unwrap().reset();
    }

    /// Get context usage stats (for diagnostics)
    pub fn context_usage(&self) -> (usize, usize) {
        self.context.read().unwrap().token_usage()
    }

    fn backend_by_id(&self, id: BackendId) -> Option<&dyn TranslatorBackend> {
        self.backends
            .iter()
            .map(|b| b.as_ref())
            .find(|b| b.id() == id)
    }

    fn is_enabled(&self, id: BackendId) -> bool {
        match id {
            BackendId::FoundryLocal => self.config.enable_foundry_local,
            BackendId::WindowsAi => self.config.enable_windows_ai,
            BackendId::OfflineMt => self.config.enable_offline_mt,
            BackendId::Mock => self.config.allow_mock_fallback,
        }
    }

    fn ordered_backend_ids(&self) -> Vec<BackendId> {
        // Fallback order:
        // 1) Foundry Local (primary), 2) Offline MT, 3) Windows AI (experimental), 4) Mock
        vec![
            BackendId::FoundryLocal,
            BackendId::OfflineMt,
            BackendId::WindowsAi,
            BackendId::Mock,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::LlmError;
    use crate::config::{OfflineMtConfig, TranslationConfig};
    use async_trait::async_trait;

    struct TestBackend {
        id: BackendId,
        available: bool,
        ready_state: ReadyState,
        response: Result<String, LlmError>,
        delay_ms: u64,
    }

    #[async_trait]
    impl TranslatorBackend for TestBackend {
        fn id(&self) -> BackendId {
            self.id
        }

        fn name(&self) -> &'static str {
            "Test"
        }

        fn is_available(&self) -> bool {
            self.available
        }

        fn ready_state(&self) -> ReadyState {
            self.ready_state
        }

        fn notes(&self) -> String {
            String::new()
        }

        async fn translate(
            &self,
            _text: &str,
            _source_language: &str,
            _target_language: &str,
        ) -> Result<String, LlmError> {
            if self.delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            }
            self.response.clone()
        }
    }

    fn base_config() -> TranslationConfig {
        TranslationConfig {
            enable_foundry_local: true,
            enable_windows_ai: true,
            enable_offline_mt: true,
            allow_mock_fallback: true,
            enable_context_aware: true,
            foundry_local: crate::config::FoundryLocalConfig::default(),
            offline_mt: OfflineMtConfig::default(),
        }
    }

    #[tokio::test]
    async fn test_fallback_ordering() {
        let backends: Vec<Box<dyn TranslatorBackend>> = vec![
            Box::new(TestBackend {
                id: BackendId::WindowsAi,
                available: true,
                ready_state: ReadyState::Ready,
                response: Err(LlmError::ApiError("boom".to_string())),
                delay_ms: 0,
            }),
            Box::new(TestBackend {
                id: BackendId::OfflineMt,
                available: true,
                ready_state: ReadyState::Ready,
                response: Ok("ok".to_string()),
                delay_ms: 0,
            }),
        ];

        let diagnostics = Arc::new(Mutex::new(TranslationDiagnosticsState::default()));
        let manager =
            TranslationManager::with_backends(base_config(), backends, diagnostics, 200);

        let outcome = manager
            .translate_with_fallback("hello", "en-US", "zh-CN")
            .await;

        assert_eq!(outcome.backend_used, BackendId::OfflineMt);
        assert_eq!(outcome.translated, "ok");
    }

    #[tokio::test]
    async fn test_backend_timeout_fallback() {
        let backends: Vec<Box<dyn TranslatorBackend>> = vec![
            Box::new(TestBackend {
                id: BackendId::FoundryLocal,
                available: true,
                ready_state: ReadyState::Ready,
                response: Ok("slow".to_string()),
                delay_ms: 50,
            }),
            Box::new(TestBackend {
                id: BackendId::OfflineMt,
                available: true,
                ready_state: ReadyState::Ready,
                response: Ok("fast".to_string()),
                delay_ms: 0,
            }),
        ];

        let diagnostics = Arc::new(Mutex::new(TranslationDiagnosticsState::default()));
        let manager = TranslationManager::with_backends(base_config(), backends, diagnostics, 10);

        let outcome = manager
            .translate_with_fallback("hello", "en-US", "zh-CN")
            .await;

        assert_eq!(outcome.backend_used, BackendId::OfflineMt);
        assert_eq!(outcome.translated, "fast");
    }
}
