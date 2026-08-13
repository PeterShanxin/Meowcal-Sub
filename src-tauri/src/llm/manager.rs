// =============================================================================
// MANAGER.RS - Translation Backend Selection + Fallback
// =============================================================================

use super::translation_attempt::{AttemptBudget, AttemptPolicy};
use super::translation_planner::{ContextTier, TieredOutcome, TieredPlan, TranslationPlanner};
use crate::config::{ContextLevel, TranslationConfig};
use crate::llm::{
    BackendId, BackendInfo, FoundryLocalBackend, MockBackend, ReadyState, TranslationContext,
    TranslationDiagnostics, TranslationDiagnosticsState, TranslationDisplayState,
    TranslationOutcome, TranslatorBackend,
};
use crate::sync_utils::{lock_or_recover, read_or_recover, write_or_recover};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;
use tauri::AppHandle;
use tokio::time::{timeout, Duration};
use tracing::{debug, info, warn};

const DEFAULT_BACKEND_TIMEOUT_MS: u64 = 2500;
/// Per-attempt ceiling with no context attached. Comfortably above the measured
/// p99 of 2291ms on a warm local model, so ordinary slow lines still land; far
/// below the total budget, so a stall is abandoned in time to retry it.
///
/// The whole chain - two attempts and the passthrough - has to finish inside
/// `pipeline_deadline::TRANSLATION_DEADLINE`, which abandons the line outright.
/// At the previous 6500 the first attempt alone outlived it, so for anyone with
/// context-aware translation off the retry and the Mock source-passthrough below
/// were unreachable: a viewer who would have seen the untranslated source line
/// saw nothing at all.
///
/// The old value was larger than the contexted cap on purpose - without context
/// there is no tier to degrade to, so a retry is the only recourse and each
/// attempt was given more room. Under an outer deadline that no longer fits, and
/// two attempts that both finish beat one that gets cut off.
const UNCONTEXTED_ATTEMPT_TIMEOUT_MS: u64 = DEFAULT_BACKEND_TIMEOUT_MS;

use crate::pipeline_deadline::backend_budget;

const MAX_TRANSLATION_INPUT_CHARS: usize = 2000;
const FOUNDRY_TRANSIENT_MAX_RETRIES: usize = 2;
const FOUNDRY_TRANSIENT_RETRY_DELAY_MS: u64 = 600;

// =============================================================================
// TRANSLATION MANAGER - Backend selection + fallback, context storage
// =============================================================================
// Context-tier progression (degradation on timeout/slow success, effective
// tier persistence) lives in `llm/translation_planner.rs`; this module owns
// the tier store, context storage, backend fallback, and display mapping.
// =============================================================================

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

    /// Record a line the pipeline gave up on before any backend answered.
    ///
    /// Every other diagnostics write happens inside the backend future, so
    /// dropping that future at the deadline skipped all of them. `last_error`
    /// and `last_latency` are last-write-wins, so a run of abandoned lines left
    /// the last *successful* line's latency standing and no error at all - the
    /// overlay saying the engine is behind while the panel showed it healthy.
    ///
    /// Attributed to the local engine because that is the call that was holding
    /// the slot, and the only one whose speed the viewer can act on.
    pub fn record_abandoned(&self, waited_ms: u128) {
        let backend = if self.config.enable_foundry_local {
            BackendId::FoundryLocal
        } else {
            BackendId::Mock
        };
        lock_or_recover(&self.diagnostics).record_error(
            backend,
            "abandoned_deadline",
            Some(waited_ms),
        );
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
            // Bounded by the pipeline's deadline as well as by config: the retry
            // loop below measures its remaining budget against this, and if it
            // measures against thirty seconds while the line is abandoned at
            // five, it starts a retry that is killed mid-flight and never
            // reaches the passthrough.
            let total_timeout = Duration::from_millis(timeout_ms).min(backend_budget());
            let max_attempts = if id == BackendId::FoundryLocal {
                1 + FOUNDRY_TRANSIENT_MAX_RETRIES
            } else {
                1
            };
            let attempt_policy = AttemptPolicy {
                max_attempts,
                retry_delay_ms: FOUNDRY_TRANSIENT_RETRY_DELAY_MS,
                contexted_attempt_cap_ms: DEFAULT_BACKEND_TIMEOUT_MS,
                uncontexted_attempt_cap_ms: UNCONTEXTED_ATTEMPT_TIMEOUT_MS,
                prompt_max_context_chars: self.config.prompt_max_context_chars,
                prompt_max_source_chars: self.config.prompt_max_source_chars,
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
                        attempt_policy,
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
        let engine_warnings: Vec<String> = warnings
            .iter()
            .filter(|warning| warning.starts_with("local_engine:"))
            .map(|warning| warning.to_ascii_lowercase())
            .collect();

        if engine_warnings
            .iter()
            .any(|warning| warning.contains("not_ready"))
        {
            return TranslationDisplayState::Warming;
        }

        if engine_warnings.iter().any(|warning| {
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
        attempt_policy: AttemptPolicy,
        warnings: &mut Vec<String>,
    ) -> Option<TranslationOutcome> {
        let started = Instant::now();
        let budget = AttemptBudget {
            started,
            total_timeout,
        };
        let planner = TranslationPlanner::new(attempt_policy, Arc::clone(&self.diagnostics));

        // Load current tier from atomic storage, and pre-build the memory-only
        // prompt here (the planner operates on prebuilt prompts only).
        let initial_tier = ContextTier::from_u8(self.context_tier.load(Ordering::SeqCst));
        let memory_only_prompt = if initial_tier >= ContextTier::MemoryOnly {
            self.context_read()
                .build_memory_prompt(self.config.prompt_max_context_chars)
        } else {
            None
        };

        let outcome = planner
            .run_tiered_sequence(
                backend,
                &TieredPlan {
                    text,
                    source_language,
                    target_language,
                    full_context_prompt,
                    memory_only_prompt: memory_only_prompt.as_deref(),
                    initial_tier,
                    tier_store: &self.context_tier,
                },
                ready_state,
                &budget,
                warnings,
            )
            .await?;

        let TieredOutcome { translated, .. } = outcome;
        Some(TranslationOutcome {
            translated,
            backend_used: backend.id(),
            warnings: std::mem::take(warnings),
            display_state: TranslationDisplayState::Translated,
        })
    }
}

#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "manager_tier_tests.rs"]
mod tier_tests;
