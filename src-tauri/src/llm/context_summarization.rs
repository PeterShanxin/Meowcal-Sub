// =============================================================================
// CONTEXT_SUMMARIZATION.RS - context compression scheduling and summarization
// =============================================================================
// The capture loop used to carry this whole machine inline: the
// needs-compression gate, the wall-clock cooldown, an in-flight flag, a
// stability delay, a generation snapshot that aborts a run when subtitles kept
// changing, the history drain, a fresh Foundry Local backend per run, a
// 3-attempt retry loop, and the restore/cap failure semantics.
//
// It lives here so each branch is a deterministic unit test instead of a path
// through a live Windows session. `TranslationManager` stays the context
// storage owner: this module drains, restores, caps, and updates memory
// through it, and never touches the storage directly.
//
// The summarizer dependency is one narrow trait. Production supplies
// `FoundryContextSummarizer` (fresh backend per run, service refresh,
// availability); tests supply a deterministic fake.
// =============================================================================

use super::TranslationManager;
use crate::config::{FoundryLocalConfig, TranslationConfig};
use crate::llm::{FoundryLocalBackend, LlmError, TranslatorBackend};
use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::watch;
use tracing::{debug, warn};

/// Maximum attempts at summarizing before the drained history is restored.
const MAX_RETRIES: usize = 3;
/// Delay between summarization attempts.
const RETRY_DELAY_MS: u64 = 500;
/// Wait after scheduling before draining history, so a run lands in a stable
/// subtitle window instead of racing the live translation loop.
const STABILITY_DELAY_MS: u64 = 900;

/// Why a summarization attempt produced nothing usable.
#[derive(Debug, Clone)]
pub(crate) enum SummarizerError {
    /// The backend is disabled or unavailable. Not retried: the drained
    /// history is restored and the budget capped, silently, as before.
    Unavailable,
    /// The backend answered with an error. Retried up to `MAX_RETRIES`.
    Failed(LlmError),
}

/// One narrow seam over the backend that turns history lines into memory.
#[async_trait]
pub(crate) trait ContextSummarizer: Send + Sync {
    /// Summarize source-only history lines into a compact memory block.
    async fn summarize(&self, history: &[String]) -> Result<String, SummarizerError>;
}

/// Production summarizer: one fresh `FoundryLocalBackend` per summarization
/// run, service discovery refreshed before first use.
pub(crate) struct FoundryContextSummarizer {
    enabled: bool,
    config: FoundryLocalConfig,
    backend: OnceLock<FoundryLocalBackend>,
}

impl FoundryContextSummarizer {
    pub(crate) fn new(config: TranslationConfig) -> Self {
        Self {
            enabled: config.enable_foundry_local,
            config: config.foundry_local,
            backend: OnceLock::new(),
        }
    }
}

#[async_trait]
impl ContextSummarizer for FoundryContextSummarizer {
    async fn summarize(&self, history: &[String]) -> Result<String, SummarizerError> {
        if !self.enabled {
            return Err(SummarizerError::Unavailable);
        }

        let backend = self.backend.get_or_init(|| {
            let backend = FoundryLocalBackend::new(self.config.clone());
            backend.refresh_service_status();
            backend
        });

        if !backend.is_available() {
            return Err(SummarizerError::Unavailable);
        }

        backend
            .summarize_context(history)
            .await
            .map_err(SummarizerError::Failed)
    }
}

/// Releases the in-flight flag however the spawned task ends.
///
/// A store placed after the `await` looks equivalent and is not: an unwind
/// skips it, and a panic in a detached task is swallowed by the runtime, so
/// the flag would stay set for the rest of the session and summarization
/// would never run again.
struct CompressionFlagGuard {
    flag: Arc<AtomicBool>,
}

impl CompressionFlagGuard {
    fn new(flag: Arc<AtomicBool>) -> Self {
        Self { flag }
    }
}

impl Drop for CompressionFlagGuard {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::SeqCst);
    }
}

/// Owns the context compression state machine for one translation session.
///
/// `commands.rs` keeps the shared generation counter (it is bumped on region
/// changes and after every translator claim, on the loop's side of the
/// handover) and supplies `now_ms`; this owner keeps the cooldown clock, the
/// in-flight lifecycle, the stability delay, and the drain/retry/restore
/// sequence.
pub(crate) struct ContextCompressionScheduler {
    manager: Arc<TranslationManager>,
    summarizer: Arc<dyn ContextSummarizer>,
    generation: Arc<AtomicU64>,
    cooldown_ms: u64,
    in_flight: Arc<AtomicBool>,
    last_scheduled_ms: AtomicU64,
}

impl ContextCompressionScheduler {
    pub(crate) fn new(
        manager: Arc<TranslationManager>,
        summarizer: Arc<dyn ContextSummarizer>,
        generation: Arc<AtomicU64>,
        cooldown_ms: u64,
    ) -> Self {
        Self {
            manager,
            summarizer,
            generation,
            cooldown_ms,
            in_flight: Arc::new(AtomicBool::new(false)),
            last_scheduled_ms: AtomicU64::new(0),
        }
    }

    /// Schedule a summarization run if the context needs compression.
    ///
    /// A run is scheduled only when compression is needed, the cooldown has
    /// passed (or is zero), and no run is in flight. The work itself happens
    /// on a spawned task so the capture loop never waits on the backend.
    pub(crate) fn schedule_if_needed(&self, now_ms: u64, stop_rx: watch::Receiver<bool>) {
        if !self.manager.needs_context_compression() {
            return;
        }

        let cooldown_ok = self.cooldown_ms == 0
            || now_ms.saturating_sub(self.last_scheduled_ms.load(Ordering::SeqCst))
                >= self.cooldown_ms;
        if !cooldown_ok {
            return;
        }

        if self.in_flight.swap(true, Ordering::SeqCst) {
            return;
        }
        self.last_scheduled_ms.store(now_ms, Ordering::SeqCst);
        debug!("Context needs compression, scheduling summarization");

        let manager = Arc::clone(&self.manager);
        let summarizer = Arc::clone(&self.summarizer);
        let generation = Arc::clone(&self.generation);
        let in_flight = Arc::clone(&self.in_flight);
        let scheduled_generation = generation.load(Ordering::SeqCst);

        tokio::spawn(async move {
            let _reset = CompressionFlagGuard::new(in_flight);

            tokio::time::sleep(Duration::from_millis(STABILITY_DELAY_MS)).await;

            if *stop_rx.borrow() {
                return;
            }

            // Abort if subtitles changed while we were waiting for an idle window.
            if generation.load(Ordering::SeqCst) != scheduled_generation {
                debug!("Skipping context summarization (text still changing)");
                return;
            }

            let history_entries = manager.get_history_for_summarization();
            if history_entries.is_empty() {
                return;
            }

            let history_lines: Vec<String> = history_entries
                .iter()
                .map(|entry| entry.text.clone())
                .collect();

            for attempt in 1..=MAX_RETRIES {
                if *stop_rx.borrow() {
                    return;
                }

                match summarizer.summarize(&history_lines).await {
                    Ok(summary) if !summary.trim().is_empty() => {
                        manager.update_context_memory(summary);
                        return;
                    }
                    Ok(_) => {
                        warn!(
                            "Context summarization attempt {} returned empty output",
                            attempt
                        );
                    }
                    Err(SummarizerError::Unavailable) => {
                        manager.restore_history_entries(history_entries);
                        manager.cap_history_to_budget();
                        return;
                    }
                    Err(SummarizerError::Failed(err)) => {
                        warn!("Context summarization attempt {} failed: {}", attempt, err);
                    }
                }

                if attempt == MAX_RETRIES {
                    manager.restore_history_entries(history_entries);
                    manager.cap_history_to_budget();
                    return;
                }

                tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
            }
        });
    }
}

#[cfg(test)]
#[path = "context_summarization_test_fixtures.rs"]
mod fixtures;

#[cfg(test)]
#[path = "context_summarization_tests.rs"]
mod tests;
