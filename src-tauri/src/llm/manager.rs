// =============================================================================
// MANAGER.RS - Translation Backend Selection + Fallback
// =============================================================================

use crate::config::TranslationConfig;
use crate::llm::{
    BackendId, BackendInfo, EdgeTranslatorBackend, MockBackend, OfflineMtBackend, PhiSilica,
    ReadyState, TranslationOutcome, TranslatorBackend,
};
use tauri::AppHandle;
use tracing::{info, warn};

/// Manages available translation backends and fallback selection
pub struct TranslationManager {
    config: TranslationConfig,
    backends: Vec<Box<dyn TranslatorBackend>>,
}

impl TranslationManager {
    /// Create a new manager with the configured backends
    pub fn new(config: TranslationConfig, app: AppHandle) -> Self {
        let mut backends: Vec<Box<dyn TranslatorBackend>> = Vec::new();

        backends.push(Box::new(PhiSilica::new()));
        backends.push(Box::new(OfflineMtBackend::new(app.clone(), config.offline_mt.clone())));
        backends.push(Box::new(EdgeTranslatorBackend::new(app)));
        backends.push(Box::new(MockBackend::new()));

        Self { config, backends }
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
                    warnings.push(format!("{}: backend not registered", id.as_str()));
                    continue;
                }
            };

            if !self.is_enabled(id) {
                warnings.push(format!("{}: disabled", id.as_str()));
                continue;
            }

            if !backend.is_available() {
                warnings.push(format!("{}: not available", id.as_str()));
                continue;
            }

            if backend.ready_state() != ReadyState::Ready {
                warnings.push(format!("{}: not ready", id.as_str()));
                continue;
            }

            match backend.translate(text, source_language, target_language).await {
                Ok(translated) => {
                    info!("Translation backend used: {}", id.as_str());
                    return TranslationOutcome {
                        translated,
                        backend_used: id,
                        warnings,
                    };
                }
                Err(err) => {
                    warn!("Backend {} failed: {}", id.as_str(), err);
                    warnings.push(format!("{}: {}", id.as_str(), err));
                }
            }
        }

        if self.config.allow_mock_fallback {
            if let Some(mock) = self.backend_by_id(BackendId::Mock) {
                match mock.translate(text, source_language, target_language).await {
                    Ok(translated) => {
                        warnings.push("mock: fallback used".to_string());
                        return TranslationOutcome {
                            translated,
                            backend_used: BackendId::Mock,
                            warnings,
                        };
                    }
                    Err(err) => {
                        warnings.push(format!("mock: {}", err));
                    }
                }
            }
        }

        // Last resort: passthrough to keep the app responsive
        warnings.push("no translation backend available".to_string());
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

        // Desired fallback order:
        // 1) Windows AI, 2) Offline MT, 3) Edge Translator, 4) Mock
        ids.extend([
            BackendId::WindowsAi,
            BackendId::OfflineMt,
            BackendId::EdgeTranslator,
            BackendId::Mock,
        ]);

        ids
    }
}
