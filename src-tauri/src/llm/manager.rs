// =============================================================================
// MANAGER.RS - Translation Backend Selection + Fallback
// =============================================================================

use crate::config::{ContextLevel, TranslationConfig};
use crate::llm::{
    BackendId, BackendInfo, FoundryLocalBackend, LlmError, MockBackend, OfflineMtBackend,
    PhiSilica, ReadyState, TranslationContext, TranslationDiagnostics, TranslationDiagnosticsState,
    TranslationOutcome, TranslatorBackend,
};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;
use tauri::AppHandle;
use tokio::time::{timeout, Duration};
use tracing::{debug, info, warn};

const DEFAULT_BACKEND_TIMEOUT_MS: u64 = 2500;
const MAX_TRANSLATION_INPUT_CHARS: usize = 2000;
const FOUNDRY_TRANSIENT_MAX_RETRIES: usize = 2;
const FOUNDRY_TRANSIENT_RETRY_DELAY_MS: u64 = 600;
const CONTEXT_TIER_FULL: u8 = 2;
const CONTEXT_TIER_MEMORY_ONLY: u8 = 1;
const CONTEXT_TIER_NONE: u8 = 0;
const CONTEXT_SLOW_DEGRADE_MS: u128 = 1800;

/// Manages available translation backends and fallback selection
pub struct TranslationManager {
    config: TranslationConfig,
    backends: Vec<Box<dyn TranslatorBackend>>,
    diagnostics: Arc<Mutex<TranslationDiagnosticsState>>,
    backend_timeout_ms: u64,
    /// Session translation context (shared across calls)
    context: Arc<RwLock<TranslationContext>>,
    /// Effective context tier to use for Foundry Local requests.
    ///
    /// 2 = memory + recent, 1 = memory only, 0 = no context
    context_tier: AtomicU8,
}

impl TranslationManager {
    /// Create a new manager with the configured backends
    pub fn new(
        config: TranslationConfig,
        app: AppHandle,
        diagnostics: Arc<Mutex<TranslationDiagnosticsState>>,
    ) -> Self {
        // Fallback order: Foundry Local -> Offline MT -> Windows AI -> Mock
        let backends: Vec<Box<dyn TranslatorBackend>> = vec![
            Box::new(FoundryLocalBackend::new(config.foundry_local.clone())),
            Box::new(OfflineMtBackend::new(
                app.clone(),
                config.offline_mt.clone(),
            )),
            Box::new(PhiSilica::new()),
            Box::new(MockBackend::new()),
        ];

        // Initialize context with auto-detected budget
        let context_budget = Self::detect_context_budget(&config);
        let context_enabled =
            config.enable_context_aware && config.context_level != ContextLevel::Off;
        let context = TranslationContext::new(context_budget, context_enabled);
        let context_tier = if context_enabled {
            match config.context_level {
                ContextLevel::Off => CONTEXT_TIER_NONE,
                ContextLevel::MemoryOnly => CONTEXT_TIER_MEMORY_ONLY,
                ContextLevel::MemoryAndRecent => CONTEXT_TIER_FULL,
            }
        } else {
            CONTEXT_TIER_NONE
        };

        Self {
            config,
            backends,
            diagnostics,
            backend_timeout_ms: DEFAULT_BACKEND_TIMEOUT_MS,
            context: Arc::new(RwLock::new(context)),
            context_tier: AtomicU8::new(context_tier),
        }
    }

    /// Detect appropriate context budget based on model
    fn detect_context_budget(config: &TranslationConfig) -> usize {
        // Try to detect from Foundry Local model if available
        if config.enable_foundry_local {
            if let Some(ref model) = config.foundry_local.model {
                if let Some(window) = FoundryLocalBackend::get_model_context_window(model) {
                    let percent = (config.context_budget_percent.clamp(5, 30) as f32) / 100.0;
                    let budget = (window as f32 * percent) as usize;
                    debug!(
                        "Context budget from model {}: {} tokens ({}%)",
                        model,
                        budget,
                        (percent * 100.0).round()
                    );
                    return budget.clamp(200, 2000);
                }
            }
            debug!("No Foundry Local model configured; using default context budget");
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
        let context_enabled =
            config.enable_context_aware && config.context_level != ContextLevel::Off;
        let context = TranslationContext::new(500, context_enabled);
        let context_tier = if context_enabled {
            match config.context_level {
                ContextLevel::Off => CONTEXT_TIER_NONE,
                ContextLevel::MemoryOnly => CONTEXT_TIER_MEMORY_ONLY,
                ContextLevel::MemoryAndRecent => CONTEXT_TIER_FULL,
            }
        } else {
            CONTEXT_TIER_NONE
        };
        Self {
            config,
            backends,
            diagnostics,
            backend_timeout_ms,
            context: Arc::new(RwLock::new(context)),
            context_tier: AtomicU8::new(context_tier),
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
        self.translate_with_context(text, source_language, target_language, None)
            .await
    }

    /// Translate with fallback chain, optionally applying LLM context
    pub async fn translate_with_context(
        &self,
        text: &str,
        source_language: &str,
        target_language: &str,
        context_prompt: Option<&str>,
    ) -> TranslationOutcome {
        if text.trim().is_empty() {
            return TranslationOutcome {
                translated: String::new(),
                backend_used: BackendId::Mock,
                warnings: Vec::new(),
            };
        }

        let context_prompt =
            if self.config.enable_context_aware && self.config.context_level != ContextLevel::Off {
                context_prompt.filter(|ctx| !ctx.trim().is_empty())
            } else {
                None
            };

        let input_chars = text.chars().count();
        if input_chars > MAX_TRANSLATION_INPUT_CHARS {
            let warning = format!("input_too_long: max {} chars", MAX_TRANSLATION_INPUT_CHARS);
            self.diagnostics
                .lock()
                .unwrap()
                .record_error(BackendId::Mock, "input_too_long", None);
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
            let timeout_ms = self.timeout_ms_for_backend(id);
            let total_timeout = Duration::from_millis(timeout_ms);
            let max_attempts = if id == BackendId::FoundryLocal {
                1 + FOUNDRY_TRANSIENT_MAX_RETRIES
            } else {
                1
            };

            // Foundry Local supports context and can degrade it on slow/timeout paths
            // to keep subtitle output responsive.
            if id == BackendId::FoundryLocal && Self::backend_supports_context(id) {
                let initial_tier = self
                    .context_tier
                    .load(Ordering::SeqCst)
                    .clamp(CONTEXT_TIER_NONE, CONTEXT_TIER_FULL);

                let memory_only_prompt = if initial_tier >= CONTEXT_TIER_MEMORY_ONLY {
                    self.context_read().build_memory_prompt()
                } else {
                    None
                };

                let mut tier = initial_tier;
                let mut last_error: Option<LlmError> = None;

                loop {
                    let context_for_tier = match tier {
                        CONTEXT_TIER_FULL => context_prompt,
                        CONTEXT_TIER_MEMORY_ONLY => memory_only_prompt.as_deref(),
                        _ => None,
                    };
                    let context_used = context_for_tier.is_some();

                    let mut attempt = 0usize;
                    let mut timed_out = false;
                    let mut total_exhausted = false;
                    loop {
                        attempt += 1;

                        let remaining_total = total_timeout.saturating_sub(started.elapsed());
                        if remaining_total.is_zero() {
                            let latency_ms = started.elapsed().as_millis();
                            let error_code = "timeout";
                            self.diagnostics.lock().unwrap().record_error(
                                id,
                                error_code,
                                Some(latency_ms),
                            );
                            warn!(
                                backend_id = id.as_str(),
                                ready_state = ?ready_state,
                                latency_ms,
                                error_code,
                                "Translation backend timed out"
                            );
                            warnings.push(format!("{}: timeout", id.as_str()));
                            timed_out = true;
                            total_exhausted = true;
                            break;
                        }

                        // Soft timeout for contextful attempts so we can fall back quickly.
                        let attempt_timeout = if tier > CONTEXT_TIER_NONE {
                            remaining_total.min(Duration::from_millis(DEFAULT_BACKEND_TIMEOUT_MS))
                        } else {
                            remaining_total
                        };

                        let result = timeout(
                            attempt_timeout,
                            backend.translate_with_context(
                                text,
                                source_language,
                                target_language,
                                context_for_tier,
                            ),
                        )
                        .await;
                        let latency_ms = started.elapsed().as_millis();

                        match result {
                            Ok(Ok(translated)) => {
                                self.diagnostics
                                    .lock()
                                    .unwrap()
                                    .record_success(id, latency_ms);
                                if attempt > 1 {
                                    warnings
                                        .push(format!("{}: recovered_after_retry", id.as_str()));
                                }

                                if context_used {
                                    if tier > CONTEXT_TIER_NONE
                                        && latency_ms > CONTEXT_SLOW_DEGRADE_MS
                                    {
                                        let degraded = tier.saturating_sub(1);
                                        if degraded != tier {
                                            self.context_tier.store(degraded, Ordering::SeqCst);
                                            warnings.push(format!(
                                                "{}: context_degraded_slow",
                                                id.as_str()
                                            ));
                                        }
                                    } else {
                                        self.context_tier.store(tier, Ordering::SeqCst);
                                    }
                                }

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
                                let should_retry = attempt < max_attempts
                                    && Self::should_retry_foundry_error(&err);

                                self.diagnostics.lock().unwrap().record_error(
                                    id,
                                    err.code(),
                                    Some(latency_ms),
                                );
                                warn!(
                                    backend_id = id.as_str(),
                                    ready_state = ?ready_state,
                                    latency_ms,
                                    error_code = err.code(),
                                    attempt,
                                    max_attempts,
                                    "Translation backend failed: {}",
                                    err
                                );
                                last_error = Some(err.clone());

                                if should_retry {
                                    let delay = Duration::from_millis(
                                        FOUNDRY_TRANSIENT_RETRY_DELAY_MS
                                            .saturating_mul(attempt as u64),
                                    );
                                    let remaining_after_delay =
                                        total_timeout.saturating_sub(started.elapsed());
                                    if remaining_after_delay > delay {
                                        tokio::time::sleep(delay).await;
                                        continue;
                                    }
                                }

                                break;
                            }
                            Err(_) => {
                                let error_code = "timeout";
                                self.diagnostics.lock().unwrap().record_error(
                                    id,
                                    error_code,
                                    Some(latency_ms),
                                );
                                warn!(
                                    backend_id = id.as_str(),
                                    ready_state = ?ready_state,
                                    latency_ms,
                                    error_code,
                                    "Translation backend timed out"
                                );
                                warnings.push(format!("{}: timeout", id.as_str()));
                                timed_out = true;
                                break;
                            }
                        }
                    }

                    // If we exhausted the overall timeout, don't keep retrying with lower tiers.
                    if total_exhausted {
                        break;
                    }

                    // Only degrade on timeouts when context was actually used. Other errors should
                    // fall through to the next backend without extra work.
                    if timed_out && context_used && tier > CONTEXT_TIER_NONE {
                        let degraded = tier.saturating_sub(1);
                        if degraded != tier {
                            tier = degraded;
                            self.context_tier.store(tier, Ordering::SeqCst);
                            warnings.push(format!("{}: context_degraded", id.as_str()));
                            continue;
                        }
                    }

                    break;
                }

                if let Some(err) = last_error {
                    warnings.push(format!("{}: {}", id.as_str(), err));
                }

                continue;
            }

            // Default behavior for non-context backends.
            let remaining = total_timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                let latency_ms = started.elapsed().as_millis();
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
                continue;
            }

            let result = timeout(
                remaining,
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
                        self.diagnostics
                            .lock()
                            .unwrap()
                            .record_success(BackendId::Mock, 0);
                        warnings.push("mock: fallback used".to_string());
                        return TranslationOutcome {
                            translated,
                            backend_used: BackendId::Mock,
                            warnings,
                        };
                    }
                    Err(err) => {
                        self.diagnostics.lock().unwrap().record_error(
                            BackendId::Mock,
                            err.code(),
                            None,
                        );
                        warnings.push(format!("mock: {}", err));
                    }
                }
            }
        }

        // Last resort: passthrough to keep the app responsive
        warnings.push("no translation backend available".to_string());
        self.diagnostics.lock().unwrap().record_error(
            BackendId::Mock,
            "no_backend_available",
            None,
        );
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
        self.context_read().is_duplicate(text)
    }

    /// Record a successful translation in context
    pub fn record_translation(&self, source: &str, translation: &str) {
        if !self.config.enable_context_aware {
            return;
        }
        self.context_write().add_translation(source, translation);
    }

    /// Get context prompt to enhance translation request
    pub fn get_context_prompt(&self) -> Option<String> {
        if !self.config.enable_context_aware || self.config.context_level == ContextLevel::Off {
            return None;
        }

        match self.context_tier.load(Ordering::SeqCst) {
            CONTEXT_TIER_FULL => self
                .context_read()
                .build_context_prompt_with_recent_limit(self.config.context_recent_count),
            CONTEXT_TIER_MEMORY_ONLY => self.context_read().build_memory_prompt(),
            _ => None,
        }
    }

    /// Check if context needs compression (memory summarization)
    pub fn needs_context_compression(&self) -> bool {
        self.context_read().needs_compression()
    }

    /// Get history entries for summarization
    pub fn get_history_for_summarization(&self) -> Vec<crate::llm::HistoryEntry> {
        self.context_write().get_history_for_summarization()
    }

    /// Restore drained history entries (used when summarization fails)
    pub fn restore_history_entries(&self, entries: Vec<crate::llm::HistoryEntry>) {
        self.context_write().restore_history_entries(entries);
    }

    /// Cap history to the current context budget (used on summarization failure)
    pub fn cap_history_to_budget(&self) {
        self.context_write().cap_history_to_budget();
    }

    /// Update context memory with summarized content
    pub fn update_context_memory(&self, memory: String) {
        self.context_write().set_memory(memory);
    }

    /// Reset context (call when capture session ends)
    pub fn reset_context(&self) {
        self.context_write().reset();
    }

    /// Get context usage stats (for diagnostics)
    pub fn context_usage(&self) -> (usize, usize) {
        self.context_read().token_usage()
    }

    fn context_read(&self) -> std::sync::RwLockReadGuard<'_, TranslationContext> {
        self.context.read().unwrap_or_else(|err| err.into_inner())
    }

    fn context_write(&self) -> std::sync::RwLockWriteGuard<'_, TranslationContext> {
        self.context.write().unwrap_or_else(|err| err.into_inner())
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

    fn timeout_ms_for_backend(&self, id: BackendId) -> u64 {
        let timeout_ms = match id {
            BackendId::FoundryLocal => self.config.foundry_local.timeout_ms as u64,
            BackendId::OfflineMt => self.config.offline_mt.timeout_ms as u64,
            BackendId::WindowsAi | BackendId::Mock => self.backend_timeout_ms,
        };
        timeout_ms.clamp(1, 120_000)
    }

    fn should_retry_foundry_error(err: &LlmError) -> bool {
        match err {
            LlmError::ApiError(message) => {
                let lower = message.to_ascii_lowercase();
                lower.contains("failed to load from epcontext model")
                    || lower.contains("qnn_backend_manager")
                    || lower.contains("onnxruntime::qnn")
                    || lower.contains("model is loading")
                    || lower.contains("connection refused")
                    || lower.contains("connection reset")
                    || lower.contains("temporarily unavailable")
            }
            _ => false,
        }
    }

    fn backend_supports_context(id: BackendId) -> bool {
        matches!(id, BackendId::FoundryLocal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ContextLevel, OfflineMtConfig, TranslationConfig};
    use crate::llm::LlmError;
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
            context_level: ContextLevel::MemoryAndRecent,
            context_recent_count: 3,
            context_budget_percent: 15,
            context_summary_cooldown_ms: 5_000,
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
        let manager = TranslationManager::with_backends(base_config(), backends, diagnostics, 200);

        let outcome = manager
            .translate_with_fallback("hello", "en-US", "zh-CN")
            .await;

        assert_eq!(outcome.backend_used, BackendId::OfflineMt);
        assert_eq!(outcome.translated, "ok");
    }

    #[tokio::test]
    async fn test_backend_timeout_fallback() {
        let mut config = base_config();
        config.foundry_local.timeout_ms = 10;
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
        let manager = TranslationManager::with_backends(config, backends, diagnostics, 10);

        let outcome = manager
            .translate_with_fallback("hello", "en-US", "zh-CN")
            .await;

        assert_eq!(outcome.backend_used, BackendId::OfflineMt);
        assert_eq!(outcome.translated, "fast");
    }
}
