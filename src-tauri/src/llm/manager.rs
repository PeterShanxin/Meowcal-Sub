// =============================================================================
// MANAGER.RS - Translation Backend Selection + Fallback
// =============================================================================

use super::output_validation::{quality_issue_message, validate_translation_output};
use crate::config::{ContextLevel, TranslationConfig};
use crate::llm::{
    BackendId, BackendInfo, FoundryLocalBackend, LlmError, MockBackend, PromptRouterOptions,
    ReadyState, TranslationContext, TranslationDiagnostics, TranslationDiagnosticsState,
    TranslationDisplayState, TranslationOutcome, TranslatorBackend,
};
use crate::sync_utils::{lock_or_recover, read_or_recover, write_or_recover};
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
const CONTEXT_SLOW_DEGRADE_MS: u128 = 1800;

// =============================================================================
// CONTEXT TIER - State machine for context degradation
// =============================================================================

/// Context tier for Foundry Local requests.
/// Degrades automatically on slow responses or timeouts to keep subtitles responsive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum ContextTier {
    /// No context included in translation requests
    None = 0,
    /// Only memory summary included (lighter weight)
    MemoryOnly = 1,
    /// Full context: memory + recent subtitle lines
    Full = 2,
}

impl ContextTier {
    /// Convert from raw u8 value (for atomic storage compatibility)
    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::None,
            1 => Self::MemoryOnly,
            _ => Self::Full,
        }
    }

    /// Convert from ContextLevel config setting
    fn from_config(level: ContextLevel, enabled: bool) -> Self {
        if !enabled {
            return Self::None;
        }
        match level {
            ContextLevel::Off => Self::None,
            ContextLevel::MemoryOnly => Self::MemoryOnly,
            ContextLevel::MemoryAndRecent => Self::Full,
        }
    }

    /// Degrade to a lower tier (returns self if already at None)
    fn degraded(self) -> Self {
        match self {
            Self::Full => Self::MemoryOnly,
            Self::MemoryOnly => Self::None,
            Self::None => Self::None,
        }
    }

    /// Check if this tier includes any context
    fn has_context(self) -> bool {
        self != Self::None
    }
}

/// Manages available translation backends and fallback selection
pub struct TranslationManager {
    config: TranslationConfig,
    backends: Vec<Box<dyn TranslatorBackend>>,
    diagnostics: Arc<Mutex<TranslationDiagnosticsState>>,
    backend_timeout_ms: u64,
    /// Session translation context (shared across calls)
    context: Arc<RwLock<TranslationContext>>,
    /// Effective context tier to use for Foundry Local requests.
    /// Stored as u8 for atomic operations, use ContextTier::from_u8() to read.
    context_tier: AtomicU8,
}

impl TranslationManager {
    /// Create a new manager with the configured backends
    pub fn new(
        config: TranslationConfig,
        _app: AppHandle,
        diagnostics: Arc<Mutex<TranslationDiagnosticsState>>,
    ) -> Self {
        // Fallback order: Foundry Local -> Mock (pass-through)
        let backends: Vec<Box<dyn TranslatorBackend>> = vec![
            Box::new(FoundryLocalBackend::new(config.foundry_local.clone())),
            Box::new(MockBackend::new()),
        ];

        // Initialize context with auto-detected budget
        let context_budget = Self::detect_context_budget(&config);
        let context_enabled =
            config.enable_context_aware && config.context_level != ContextLevel::Off;
        let context = TranslationContext::new(context_budget, context_enabled);
        let context_tier = ContextTier::from_config(config.context_level, context_enabled);

        Self {
            config,
            backends,
            diagnostics,
            backend_timeout_ms: DEFAULT_BACKEND_TIMEOUT_MS,
            context: Arc::new(RwLock::new(context)),
            context_tier: AtomicU8::new(context_tier as u8),
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
        let context_tier = ContextTier::from_config(config.context_level, context_enabled);
        Self {
            config,
            backends,
            diagnostics,
            backend_timeout_ms,
            context: Arc::new(RwLock::new(context)),
            context_tier: AtomicU8::new(context_tier as u8),
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

                // Add phase for Foundry Local (fast check, no probe)
                let phase = if backend.id() == BackendId::FoundryLocal && enabled {
                    // Safe downcast to get phase - we know it's a FoundryLocalBackend
                    // Use phase() which doesn't perform a network probe
                    let foundry = FoundryLocalBackend::new(self.config.foundry_local.clone());
                    foundry.refresh_service_status();
                    Some(foundry.phase())
                } else {
                    None
                };

                BackendInfo {
                    id: backend.id(),
                    name: backend.name().to_string(),
                    available,
                    ready_state,
                    notes,
                    phase,
                }
            })
            .collect()
    }

    /// Return diagnostics snapshot for frontend.
    pub fn diagnostics_snapshot(&self) -> TranslationDiagnostics {
        let backends = self.list_backends();
        let (last_error_by_backend, last_latency_by_backend) =
            lock_or_recover(&self.diagnostics).snapshot();

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
                display_state: TranslationDisplayState::SourceOnly,
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
                display_state: TranslationDisplayState::SourceOnly,
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
            if !self.is_enabled(id) {
                warnings.push(format!("{}: disabled", id.as_str()));
                lock_or_recover(&self.diagnostics).record_error(id, "disabled", None);
                debug!(
                    backend_id = id.as_str(),
                    error_code = "disabled",
                    "Translation backend skipped"
                );
                continue;
            }

            let backend = match self.backend_by_id(id) {
                Some(b) => b,
                None => {
                    warnings.push(format!("{}: backend_not_registered", id.as_str()));
                    lock_or_recover(&self.diagnostics).record_error(
                        id,
                        "backend_not_registered",
                        None,
                    );
                    continue;
                }
            };

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
                if let Some(outcome) = self
                    .try_foundry_with_context(
                        backend,
                        text,
                        source_language,
                        target_language,
                        context_prompt,
                        ready_state,
                        total_timeout,
                        max_attempts,
                        &mut warnings,
                    )
                    .await
                {
                    return outcome;
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
                    let display_state = if id == BackendId::Mock {
                        Self::fallback_display_state(&warnings)
                    } else {
                        TranslationDisplayState::Translated
                    };
                    return TranslationOutcome {
                        translated,
                        backend_used: id,
                        warnings,
                        display_state,
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
                        let display_state = Self::fallback_display_state(&warnings);
                        return TranslationOutcome {
                            translated,
                            backend_used: BackendId::Mock,
                            warnings,
                            display_state,
                        };
                    }
                    Err(err) => {
                        lock_or_recover(&self.diagnostics).record_error(
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
        lock_or_recover(&self.diagnostics).record_error(
            BackendId::Mock,
            "no_backend_available",
            None,
        );
        let display_state = Self::fallback_display_state(&warnings);
        TranslationOutcome {
            translated: text.to_string(),
            backend_used: BackendId::Mock,
            warnings,
            display_state,
        }
    }

    fn fallback_display_state(warnings: &[String]) -> TranslationDisplayState {
        let foundry_warnings: Vec<String> = warnings
            .iter()
            .filter(|warning| warning.starts_with("foundry_local:"))
            .map(|warning| warning.to_ascii_lowercase())
            .collect();

        if foundry_warnings
            .iter()
            .any(|warning| warning.contains("not_ready"))
        {
            return TranslationDisplayState::Warming;
        }

        if foundry_warnings.iter().any(|warning| {
            !warning.contains("disabled")
                && !warning.contains("not_available")
                && !warning.contains("fallback used")
        }) {
            return TranslationDisplayState::TemporarilyUnavailable;
        }

        TranslationDisplayState::SourceOnly
    }

    /// Check if text is duplicate (for deduplication in capture loop)
    pub fn is_duplicate(&self, text: &str) -> bool {
        if !self.config.enable_context_aware {
            return false;
        }
        self.context_read().is_duplicate(text)
    }

    /// Record a successful translation in context
    pub fn record_ocr_line(&self, source_text: &str) {
        if !self.config.enable_context_aware {
            return;
        }

        let reset_gap = Duration::from_millis(self.config.context_reset_gap_ms as u64);
        self.context_write().add_ocr_line(
            source_text,
            Instant::now(),
            self.config.context_buffer_size,
            self.config.prompt_max_source_chars,
            reset_gap,
        );
    }

    /// Get context prompt to enhance translation request
    pub fn get_context_prompt(&self) -> Option<String> {
        if !self.config.enable_context_aware || self.config.context_level == ContextLevel::Off {
            return None;
        }

        match ContextTier::from_u8(self.context_tier.load(Ordering::SeqCst)) {
            ContextTier::Full => self.context_read().build_context_prompt_with_recent_limit(
                self.config.context_recent_count,
                self.config.prompt_max_context_chars,
            ),
            ContextTier::MemoryOnly => self
                .context_read()
                .build_memory_prompt(self.config.prompt_max_context_chars),
            ContextTier::None => None,
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
        read_or_recover(&self.context)
    }

    fn context_write(&self) -> std::sync::RwLockWriteGuard<'_, TranslationContext> {
        write_or_recover(&self.context)
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
            BackendId::Mock => self.config.allow_mock_fallback,
        }
    }

    fn ordered_backend_ids(&self) -> Vec<BackendId> {
        // Fallback order: Foundry Local -> Mock (pass-through)
        vec![BackendId::FoundryLocal, BackendId::Mock]
    }

    fn timeout_ms_for_backend(&self, id: BackendId) -> u64 {
        let timeout_ms = match id {
            BackendId::FoundryLocal => self.config.foundry_local.timeout_ms as u64,
            BackendId::Mock => self.backend_timeout_ms,
        };
        timeout_ms.clamp(1, 120_000)
    }

    fn should_retry_foundry_error(err: &LlmError) -> bool {
        match err {
            LlmError::ApiError(message) => {
                let lower = message.to_ascii_lowercase();
                // Treat missing/permissioned ep_cache_context as non-retriable: it's unlikely to
                // resolve within a subtitle frame budget and retries just add latency.
                if lower.contains("ep_cache_context")
                    && (lower.contains("does not exist") || lower.contains("not accessible"))
                {
                    return false;
                }

                lower.contains("model is loading")
                    || lower.contains("connection refused")
                    || lower.contains("connection reset")
                    || lower.contains("temporarily unavailable")
                    || lower.contains("qnn_backend_manager")
                    || lower.contains("onnxruntime::qnn")
                    || lower.contains("failed to load from epcontext model")
            }
            _ => false,
        }
    }

    fn backend_supports_context(id: BackendId) -> bool {
        matches!(id, BackendId::FoundryLocal)
    }

    /// Attempt translation with Foundry Local, degrading context tier on slow responses/timeouts.
    ///
    /// Returns `Some(outcome)` on success, `None` if all context tiers failed (caller should
    /// try next backend in fallback chain).
    #[allow(clippy::too_many_arguments)]
    async fn try_foundry_with_context(
        &self,
        backend: &dyn TranslatorBackend,
        text: &str,
        source_language: &str,
        target_language: &str,
        full_context_prompt: Option<&str>,
        ready_state: ReadyState,
        total_timeout: Duration,
        max_attempts: usize,
        warnings: &mut Vec<String>,
    ) -> Option<TranslationOutcome> {
        let id = backend.id();
        let started = Instant::now();

        // Load current tier from atomic storage
        let initial_tier = ContextTier::from_u8(self.context_tier.load(Ordering::SeqCst));

        // Pre-build memory-only prompt if we might need it
        let memory_only_prompt = if initial_tier >= ContextTier::MemoryOnly {
            self.context_read()
                .build_memory_prompt(self.config.prompt_max_context_chars)
        } else {
            None
        };

        let mut tier = initial_tier;
        let mut last_error: Option<LlmError> = None;

        // Context degradation loop: try current tier, degrade on timeout, repeat
        loop {
            let context_for_tier =
                tier.select_context(full_context_prompt, memory_only_prompt.as_deref());
            let context_used = context_for_tier.is_some();

            // Retry loop for transient errors at current tier
            let result = self
                .try_translate_with_retries(
                    backend,
                    text,
                    source_language,
                    target_language,
                    context_for_tier,
                    context_used,
                    ready_state,
                    &started,
                    total_timeout,
                    max_attempts,
                    warnings,
                )
                .await;

            match result {
                TierAttemptResult::Success(translated, latency_ms, recovered) => {
                    if recovered {
                        warnings.push(format!("{}: recovered_after_retry", id.as_str()));
                    }

                    // If response was slow, degrade tier for future requests.
                    // Only update stored tier when context was actually used - otherwise we
                    // haven't verified that the tier works with context.
                    if context_used && tier.has_context() && latency_ms > CONTEXT_SLOW_DEGRADE_MS {
                        let degraded = tier.degraded();
                        if degraded != tier {
                            self.context_tier.store(degraded as u8, Ordering::SeqCst);
                            warnings.push(format!("{}: context_degraded_slow", id.as_str()));
                        }
                    } else if context_used {
                        self.context_tier.store(tier as u8, Ordering::SeqCst);
                    }

                    info!(
                        backend_id = id.as_str(),
                        ready_state = ?ready_state,
                        latency_ms,
                        error_code = "",
                        "Translation backend used"
                    );

                    return Some(TranslationOutcome {
                        translated,
                        backend_used: id,
                        warnings: std::mem::take(warnings),
                        display_state: TranslationDisplayState::Translated,
                    });
                }
                TierAttemptResult::Timeout { total_exhausted } => {
                    // If overall timeout exhausted, stop trying lower tiers
                    if total_exhausted {
                        break;
                    }

                    // Only degrade on timeouts when context was actually used
                    if context_used && tier.has_context() {
                        let degraded = tier.degraded();
                        if degraded != tier {
                            tier = degraded;
                            self.context_tier.store(tier as u8, Ordering::SeqCst);
                            warnings.push(format!("{}: context_degraded", id.as_str()));
                            continue; // Try again with lower tier
                        }
                    }
                    break;
                }
                TierAttemptResult::Error(err) => {
                    last_error = Some(err);
                    break;
                }
            }
        }

        if let Some(err) = last_error {
            warnings.push(format!("{}: {}", id.as_str(), err));
        }

        None // Signal caller to try next backend
    }

    /// Attempt translation with retries for transient errors.
    #[allow(clippy::too_many_arguments)]
    async fn try_translate_with_retries(
        &self,
        backend: &dyn TranslatorBackend,
        text: &str,
        source_language: &str,
        target_language: &str,
        context_prompt: Option<&str>,
        context_used: bool,
        ready_state: ReadyState,
        started: &Instant,
        total_timeout: Duration,
        max_attempts: usize,
        warnings: &mut Vec<String>,
    ) -> TierAttemptResult {
        let id = backend.id();

        for attempt in 1..=max_attempts {
            let remaining_total = total_timeout.saturating_sub(started.elapsed());
            if remaining_total.is_zero() {
                let latency_ms = started.elapsed().as_millis();
                lock_or_recover(&self.diagnostics).record_error(id, "timeout", Some(latency_ms));
                warn!(
                    backend_id = id.as_str(),
                    ready_state = ?ready_state,
                    latency_ms,
                    error_code = "timeout",
                    "Translation backend timed out"
                );
                warnings.push(format!("{}: timeout", id.as_str()));
                return TierAttemptResult::Timeout {
                    total_exhausted: true,
                };
            }

            // Soft timeout only when context is included
            let attempt_timeout = if context_used {
                remaining_total.min(Duration::from_millis(DEFAULT_BACKEND_TIMEOUT_MS))
            } else {
                remaining_total
            };

            let result = timeout(
                attempt_timeout,
                backend.translate_with_context_options(
                    text,
                    source_language,
                    target_language,
                    context_prompt,
                    Some(PromptRouterOptions {
                        enable_context: context_used,
                        max_context_chars: self.config.prompt_max_context_chars,
                        max_source_chars: self.config.prompt_max_source_chars,
                    }),
                ),
            )
            .await;
            let latency_ms = started.elapsed().as_millis();

            match result {
                Ok(Ok(translated)) => {
                    if let Err(reason) = validate_translation_output(
                        text,
                        &translated,
                        source_language,
                        target_language,
                    ) {
                        lock_or_recover(&self.diagnostics).record_error(
                            id,
                            "low_quality_output",
                            Some(latency_ms),
                        );
                        warn!(
                            backend_id = id.as_str(),
                            ready_state = ?ready_state,
                            latency_ms,
                            error_code = "low_quality_output",
                            quality_issue = reason.code(),
                            attempt,
                            max_attempts,
                            "Translation output rejected"
                        );

                        return TierAttemptResult::Error(LlmError::TranslationError(
                            quality_issue_message(reason),
                        ));
                    }

                    self.diagnostics
                        .lock()
                        .unwrap()
                        .record_success(id, latency_ms);
                    return TierAttemptResult::Success(translated, latency_ms, attempt > 1);
                }
                Ok(Err(err)) => {
                    let should_retry =
                        attempt < max_attempts && Self::should_retry_foundry_error(&err);
                    lock_or_recover(&self.diagnostics).record_error(
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

                    if should_retry {
                        let delay = Duration::from_millis(
                            FOUNDRY_TRANSIENT_RETRY_DELAY_MS.saturating_mul(attempt as u64),
                        );
                        let remaining = total_timeout.saturating_sub(started.elapsed());
                        if remaining > delay {
                            tokio::time::sleep(delay).await;
                            continue;
                        }
                    }

                    return TierAttemptResult::Error(err);
                }
                Err(_) => {
                    lock_or_recover(&self.diagnostics).record_error(
                        id,
                        "timeout",
                        Some(latency_ms),
                    );
                    warn!(
                        backend_id = id.as_str(),
                        ready_state = ?ready_state,
                        latency_ms,
                        error_code = "timeout",
                        "Translation backend timed out"
                    );
                    warnings.push(format!("{}: timeout", id.as_str()));
                    return TierAttemptResult::Timeout {
                        total_exhausted: false,
                    };
                }
            }
        }

        // Should not reach here, but fallback
        TierAttemptResult::Timeout {
            total_exhausted: true,
        }
    }
}

/// Result of attempting translation at a specific context tier
enum TierAttemptResult {
    /// Translation succeeded: (text, latency_ms, recovered_after_retry)
    Success(String, u128, bool),
    /// Timed out: total_exhausted indicates if overall timeout was hit
    Timeout { total_exhausted: bool },
    /// Non-retryable error occurred
    Error(LlmError),
}

impl ContextTier {
    /// Select the appropriate context prompt for this tier
    fn select_context<'a>(
        self,
        full_context: Option<&'a str>,
        memory_only: Option<&'a str>,
    ) -> Option<&'a str> {
        match self {
            Self::Full => full_context,
            Self::MemoryOnly => memory_only,
            Self::None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ContextLevel, TranslationConfig};
    use crate::llm::LlmError;
    use async_trait::async_trait;
    use std::sync::atomic::AtomicUsize;

    struct TestBackend {
        id: BackendId,
        available: bool,
        ready_state: ReadyState,
        response: Result<String, LlmError>,
        delay_ms: u64,
    }

    struct CountingBackend {
        calls: Arc<AtomicUsize>,
        response: Result<String, LlmError>,
    }

    #[async_trait]
    impl TranslatorBackend for CountingBackend {
        fn id(&self) -> BackendId {
            BackendId::FoundryLocal
        }

        fn name(&self) -> &'static str {
            "Counting Foundry"
        }

        fn is_available(&self) -> bool {
            true
        }

        fn ready_state(&self) -> ReadyState {
            ReadyState::Ready
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
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.response.clone()
        }
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
            allow_mock_fallback: true,
            enable_context_aware: true,
            context_level: ContextLevel::MemoryAndRecent,
            context_recent_count: 3,
            context_budget_percent: 15,
            context_summary_cooldown_ms: 5_000,
            prompt_max_source_chars: 300,
            prompt_max_context_chars: 600,
            context_buffer_size: 12,
            context_reset_gap_ms: 6_000,
            foundry_local: crate::config::FoundryLocalConfig::default(),
            ocr: crate::config::OcrConfig::default(),
        }
    }

    #[tokio::test]
    async fn test_fallback_ordering() {
        // Test fallback from FoundryLocal (fails) to Mock
        let backends: Vec<Box<dyn TranslatorBackend>> = vec![
            Box::new(TestBackend {
                id: BackendId::FoundryLocal,
                available: true,
                ready_state: ReadyState::Ready,
                response: Err(LlmError::ApiError("boom".to_string())),
                delay_ms: 0,
            }),
            Box::new(TestBackend {
                id: BackendId::Mock,
                available: true,
                ready_state: ReadyState::Ready,
                response: Ok("mock_response".to_string()),
                delay_ms: 0,
            }),
        ];

        let diagnostics = Arc::new(Mutex::new(TranslationDiagnosticsState::default()));
        let manager = TranslationManager::with_backends(base_config(), backends, diagnostics, 200);

        let outcome = manager
            .translate_with_fallback("hello", "en-US", "zh-CN")
            .await;

        assert_eq!(outcome.backend_used, BackendId::Mock);
        assert_eq!(outcome.translated, "mock_response");
        assert_eq!(
            outcome.display_state,
            TranslationDisplayState::TemporarilyUnavailable
        );
    }

    #[tokio::test]
    async fn test_backend_timeout_fallback() {
        let mut config = base_config();
        config.foundry_local.timeout_ms = 10;
        // Test timeout fallback from FoundryLocal to Mock
        let backends: Vec<Box<dyn TranslatorBackend>> = vec![
            Box::new(TestBackend {
                id: BackendId::FoundryLocal,
                available: true,
                ready_state: ReadyState::Ready,
                response: Ok("slow".to_string()),
                delay_ms: 50,
            }),
            Box::new(TestBackend {
                id: BackendId::Mock,
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

        assert_eq!(outcome.backend_used, BackendId::Mock);
        assert_eq!(outcome.translated, "fast");
        assert_eq!(
            outcome.display_state,
            TranslationDisplayState::TemporarilyUnavailable
        );
    }

    #[tokio::test]
    async fn test_zh_cn_to_en_validation_rejection_is_not_retried() {
        let calls = Arc::new(AtomicUsize::new(0));
        let backends: Vec<Box<dyn TranslatorBackend>> = vec![
            Box::new(CountingBackend {
                calls: Arc::clone(&calls),
                response: Ok("a".repeat(150)),
            }),
            Box::new(TestBackend {
                id: BackendId::Mock,
                available: true,
                ready_state: ReadyState::Ready,
                response: Ok("你好".to_string()),
                delay_ms: 0,
            }),
        ];

        let diagnostics = Arc::new(Mutex::new(TranslationDiagnosticsState::default()));
        let manager = TranslationManager::with_backends(base_config(), backends, diagnostics, 500);
        let outcome = manager
            .translate_with_fallback("你好", "zh-CN", "en-US")
            .await;

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            outcome.display_state,
            TranslationDisplayState::TemporarilyUnavailable
        );
        assert_eq!(outcome.translated, "你好");
    }

    #[tokio::test]
    async fn test_not_ready_foundry_reports_warming_without_translation() {
        let backends: Vec<Box<dyn TranslatorBackend>> = vec![
            Box::new(TestBackend {
                id: BackendId::FoundryLocal,
                available: true,
                ready_state: ReadyState::NotReady,
                response: Ok("unused".to_string()),
                delay_ms: 0,
            }),
            Box::new(TestBackend {
                id: BackendId::Mock,
                available: true,
                ready_state: ReadyState::Ready,
                response: Ok("你好".to_string()),
                delay_ms: 0,
            }),
        ];

        let diagnostics = Arc::new(Mutex::new(TranslationDiagnosticsState::default()));
        let manager = TranslationManager::with_backends(base_config(), backends, diagnostics, 500);
        let outcome = manager
            .translate_with_fallback("你好", "zh-CN", "en-US")
            .await;

        assert_eq!(outcome.display_state, TranslationDisplayState::Warming);
    }

    #[tokio::test]
    async fn test_disabled_foundry_reports_source_only() {
        let mut config = base_config();
        config.enable_foundry_local = false;
        let backends: Vec<Box<dyn TranslatorBackend>> = vec![Box::new(TestBackend {
            id: BackendId::Mock,
            available: true,
            ready_state: ReadyState::Ready,
            response: Ok("你好".to_string()),
            delay_ms: 0,
        })];

        let diagnostics = Arc::new(Mutex::new(TranslationDiagnosticsState::default()));
        let manager = TranslationManager::with_backends(config, backends, diagnostics, 500);
        let outcome = manager
            .translate_with_fallback("你好", "zh-CN", "en-US")
            .await;

        assert_eq!(outcome.display_state, TranslationDisplayState::SourceOnly);
    }
}
