// =============================================================================
// TRANSLATION_PLANNER.RS - Context-tier progression above one attempt
// =============================================================================
// Owns the context-tier state machine that runs above one
// `TranslationAttemptRunner` call: try the effective tier, degrade it on
// timeout, persist the effective tier for the next line, degrade it on slow
// success. It is deliberately knob-driven - no config, no context storage, no
// backend registry - so the manager passes prebuilt prompts, the policy, the
// shared budget, and a handle to the tier store.
//
// Backend progression (which backends, fallback order, enable/ready gating)
// stays in `TranslationManager`; this module owns only the tier progression of
// one Foundry Local sequence.
// =============================================================================

use super::translation_attempt::{
    AttemptBudget, AttemptOutcome, AttemptPolicy, AttemptRequest, TranslationAttemptRunner,
};
use crate::config::ContextLevel;
use crate::llm::{LlmError, ReadyState, TranslationDiagnosticsState, TranslatorBackend};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use tracing::info;

/// A success slower than this (ms, measured against the shared budget clock)
/// degrades the stored context tier for future requests.
pub(super) const CONTEXT_SLOW_DEGRADE_MS: u128 = 1800;

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
    pub(super) fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::None,
            1 => Self::MemoryOnly,
            _ => Self::Full,
        }
    }

    /// Convert from ContextLevel config setting
    pub(super) fn from_config(level: ContextLevel, enabled: bool) -> Self {
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
    pub(super) fn degraded(self) -> Self {
        match self {
            Self::Full => Self::MemoryOnly,
            Self::MemoryOnly => Self::None,
            Self::None => Self::None,
        }
    }

    /// Check if this tier includes any context
    pub(super) fn has_context(self) -> bool {
        self != Self::None
    }

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

/// Runs the context-tier progression for one Foundry Local sequence.
pub(super) struct TranslationPlanner {
    policy: AttemptPolicy,
    diagnostics: Arc<Mutex<TranslationDiagnosticsState>>,
    slow_degrade_ms: u128,
}

/// What one tiered sequence should translate, and where its effective tier
/// lives. `tier_store` is a handle: storage stays owned by `TranslationManager`
/// (the capture loop reads it through `get_context_prompt`), the planner only
/// writes it at the same points the tier loop always did.
pub(super) struct TieredPlan<'a> {
    pub(super) text: &'a str,
    pub(super) source_language: &'a str,
    pub(super) target_language: &'a str,
    pub(super) full_context_prompt: Option<&'a str>,
    pub(super) memory_only_prompt: Option<&'a str>,
    pub(super) initial_tier: ContextTier,
    pub(super) tier_store: &'a AtomicU8,
}

/// A line the tiered sequence translated.
#[derive(Debug)]
pub(super) struct TieredOutcome {
    pub(super) translated: String,
}

impl TranslationPlanner {
    pub(super) fn new(
        policy: AttemptPolicy,
        diagnostics: Arc<Mutex<TranslationDiagnosticsState>>,
    ) -> Self {
        Self {
            policy,
            diagnostics,
            slow_degrade_ms: CONTEXT_SLOW_DEGRADE_MS,
        }
    }

    /// Test-only constructor with a short slow-degrade threshold so the
    /// slow-success path is exercised without a long real sleep.
    #[cfg(test)]
    pub(super) fn with_slow_degrade_ms(
        policy: AttemptPolicy,
        diagnostics: Arc<Mutex<TranslationDiagnosticsState>>,
        slow_degrade_ms: u128,
    ) -> Self {
        Self {
            policy,
            diagnostics,
            slow_degrade_ms,
        }
    }

    /// Run the tier progression for one backend sequence.
    ///
    /// Returns `Some(outcome)` on success, `None` if all context tiers failed
    /// (the caller should try the next backend in the fallback chain).
    pub(super) async fn run_tiered_sequence(
        &self,
        backend: &dyn TranslatorBackend,
        plan: &TieredPlan<'_>,
        ready_state: ReadyState,
        budget: &AttemptBudget,
        warnings: &mut Vec<String>,
    ) -> Option<TieredOutcome> {
        let id = backend.id();
        let runner =
            TranslationAttemptRunner::new(self.policy.clone(), Arc::clone(&self.diagnostics));

        let mut tier = plan.initial_tier;
        let mut last_error: Option<LlmError> = None;

        // Context degradation loop: try current tier, degrade on timeout, repeat
        loop {
            let context_for_tier =
                tier.select_context(plan.full_context_prompt, plan.memory_only_prompt);
            let context_used = context_for_tier.is_some();

            // Retry loop for transient errors at current tier
            let result = runner
                .run(
                    backend,
                    &AttemptRequest {
                        text: plan.text,
                        source_language: plan.source_language,
                        target_language: plan.target_language,
                        context_prompt: context_for_tier,
                        context_used,
                    },
                    budget,
                    ready_state,
                    warnings,
                )
                .await;

            match result {
                AttemptOutcome::Succeeded {
                    translated,
                    latency_ms,
                    recovered_after_retry,
                } => {
                    if recovered_after_retry {
                        warnings.push(format!("{}: recovered_after_retry", id.as_str()));
                    }

                    // If response was slow, degrade tier for future requests.
                    // Only update stored tier when context was actually used - otherwise we
                    // haven't verified that the tier works with context.
                    if context_used && tier.has_context() && latency_ms > self.slow_degrade_ms {
                        let degraded = tier.degraded();
                        if degraded != tier {
                            plan.tier_store.store(degraded as u8, Ordering::SeqCst);
                            warnings.push(format!("{}: context_degraded_slow", id.as_str()));
                        }
                    } else if context_used {
                        plan.tier_store.store(tier as u8, Ordering::SeqCst);
                    }

                    info!(
                        backend_id = id.as_str(),
                        ready_state = ?ready_state,
                        latency_ms,
                        error_code = "",
                        "Translation backend used"
                    );

                    return Some(TieredOutcome { translated });
                }
                AttemptOutcome::TimedOut { total_exhausted } => {
                    // If overall timeout exhausted, stop trying lower tiers
                    if total_exhausted {
                        break;
                    }

                    // Only degrade on timeouts when context was actually used
                    if context_used && tier.has_context() {
                        let degraded = tier.degraded();
                        if degraded != tier {
                            tier = degraded;
                            plan.tier_store.store(tier as u8, Ordering::SeqCst);
                            warnings.push(format!("{}: context_degraded", id.as_str()));
                            continue; // Try again with lower tier
                        }
                    }
                    break;
                }
                AttemptOutcome::Failed(err) => {
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
}

#[cfg(test)]
#[path = "translation_planner_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "translation_planner_exhaustion_tests.rs"]
mod exhaustion_tests;
