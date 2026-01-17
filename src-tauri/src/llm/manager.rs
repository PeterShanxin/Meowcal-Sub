// =============================================================================
// MANAGER.RS - Translation Backend Selection + Fallback
// =============================================================================

use crate::config::TranslationConfig;
use crate::llm::{
    BackendId, BackendInfo, EdgeTranslatorBackend, FoundryLocalBackend, MockBackend,
    OfflineMtBackend, PhiSilica, ReadyState, TranslationDiagnostics, TranslationDiagnosticsState,
    TranslationOutcome, TranslatorBackend,
};
use std::sync::{Arc, Mutex};
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
}

impl TranslationManager {
    /// Create a new manager with the configured backends
    pub fn new(
        config: TranslationConfig,
        app: AppHandle,
        diagnostics: Arc<Mutex<TranslationDiagnosticsState>>,
    ) -> Self {
        let mut backends: Vec<Box<dyn TranslatorBackend>> = Vec::new();

        // Fallback order: Foundry Local -> Offline MT -> Windows AI -> Edge -> Mock
        backends.push(Box::new(FoundryLocalBackend::new(config.foundry_local.clone())));
        backends.push(Box::new(OfflineMtBackend::new(app.clone(), config.offline_mt.clone())));
        backends.push(Box::new(PhiSilica::new()));
        backends.push(Box::new(EdgeTranslatorBackend::new(app)));
        backends.push(Box::new(MockBackend::new()));

        Self {
            config,
            backends,
            diagnostics,
            backend_timeout_ms: DEFAULT_BACKEND_TIMEOUT_MS,
        }
    }

    #[cfg(test)]
    pub fn with_backends(
        config: TranslationConfig,
        backends: Vec<Box<dyn TranslatorBackend>>,
        diagnostics: Arc<Mutex<TranslationDiagnosticsState>>,
        backend_timeout_ms: u64,
    ) -> Self {
        Self {
            config,
            backends,
            diagnostics,
            backend_timeout_ms,
        }
    }

    /// List backend status for UI/diagnostics
    pub fn list_backends(&self) -> Vec<BackendInfo> {
        self.backends
            .iter()
            .map(|backend| {
                let enabled = self.is_enabled(backend.id());
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
                    available: enabled && backend.is_available(),
                    ready_state: if enabled {
                        backend.ready_state()
                    } else {
                        ReadyState::NotSupported
                    },
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
            BackendId::EdgeTranslator => self.config.enable_edge_translator,
            BackendId::Mock => self.config.allow_mock_fallback,
        }
    }

    fn preferred_backend(&self) -> Option<BackendId> {
        if self.config.preferred_backend.trim().is_empty() {
            return None;
        }

        if self.config.preferred_backend.eq_ignore_ascii_case("auto") {
            return None;
        }

        BackendId::from_str(&self.config.preferred_backend)
    }

    fn ordered_backend_ids(&self) -> Vec<BackendId> {
        let mut ids = Vec::new();

        if let Some(preferred) = self.preferred_backend() {
            ids.push(preferred);
        }

        // Fallback order:
        // 1) Foundry Local (primary), 2) Offline MT, 3) Windows AI (experimental), 4) Mock
        // Edge Translator is deprecated and disabled by default
        ids.extend([
            BackendId::FoundryLocal,
            BackendId::OfflineMt,
            BackendId::WindowsAi,
            BackendId::EdgeTranslator,
            BackendId::Mock,
        ]);

        ids
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
            preferred_backend: "auto".to_string(),
            enable_foundry_local: true,
            enable_windows_ai: true,
            enable_offline_mt: true,
            enable_edge_translator: true,
            allow_mock_fallback: true,
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
            Box::new(TestBackend {
                id: BackendId::EdgeTranslator,
                available: true,
                ready_state: ReadyState::Ready,
                response: Ok("edge".to_string()),
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
    async fn test_preferred_backend() {
        let backends: Vec<Box<dyn TranslatorBackend>> = vec![
            Box::new(TestBackend {
                id: BackendId::WindowsAi,
                available: true,
                ready_state: ReadyState::Ready,
                response: Ok("win".to_string()),
                delay_ms: 0,
            }),
            Box::new(TestBackend {
                id: BackendId::EdgeTranslator,
                available: true,
                ready_state: ReadyState::Ready,
                response: Ok("edge".to_string()),
                delay_ms: 0,
            }),
        ];

        let diagnostics = Arc::new(Mutex::new(TranslationDiagnosticsState::default()));
        let mut config = base_config();
        config.preferred_backend = "edge_translator".to_string();

        let manager = TranslationManager::with_backends(config, backends, diagnostics, 200);
        let outcome = manager
            .translate_with_fallback("hello", "en-US", "zh-CN")
            .await;

        assert_eq!(outcome.backend_used, BackendId::EdgeTranslator);
        assert_eq!(outcome.translated, "edge");
    }

    #[tokio::test]
    async fn test_backend_timeout_fallback() {
        let backends: Vec<Box<dyn TranslatorBackend>> = vec![
            Box::new(TestBackend {
                id: BackendId::WindowsAi,
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
