use super::*;
use crate::config::{ContextLevel, FoundryLocalConfig, OcrConfig, TranslationConfig};
use crate::llm::{TranslationDiagnosticsState, TranslationManager};
use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;

// Five lines of 312 chars each fill the 500-token test context past the 70%
// compression threshold (390 estimated tokens > 350) with a real
// `TranslationManager` and its real `TranslationContext` storage.
pub(super) const FILLER: usize = 300;
pub(super) const LINE_TOKENS: usize = 78;
pub(super) const FIVE_LINES_TOKENS: usize = 5 * LINE_TOKENS;
pub(super) const DRAINED_TOKENS: usize = 3 * LINE_TOKENS;

pub(super) fn test_config() -> TranslationConfig {
    TranslationConfig {
        enable_foundry_local: true,
        allow_mock_fallback: true,
        enable_context_aware: true,
        context_level: ContextLevel::MemoryAndRecent,
        context_recent_count: 3,
        context_budget_percent: 15,
        context_summary_cooldown_ms: 5_000,
        prompt_max_source_chars: 1_000,
        prompt_max_context_chars: 600,
        context_buffer_size: 12,
        context_reset_gap_ms: 60_000,
        foundry_local: FoundryLocalConfig::default(),
        ocr: OcrConfig::default(),
    }
}

// Fewer, much longer lines: three 2 000-char lines exceed the threshold while
// the drain keeps all three, so the scheduler sees an empty drain batch.
pub(super) fn test_config_large_lines() -> TranslationConfig {
    let mut config = test_config();
    config.prompt_max_source_chars = 10_000;
    config
}

pub(super) fn manager(config: TranslationConfig) -> Arc<TranslationManager> {
    Arc::new(TranslationManager::with_backends(
        config,
        Vec::new(),
        Arc::new(Mutex::new(TranslationDiagnosticsState::default())),
        500,
    ))
}

pub(super) fn fill(manager: &TranslationManager, count: usize, filler: usize) {
    for i in 0..count {
        manager.record_ocr_line(&format!("line {:05} {}", i, "a".repeat(filler)));
    }
}

pub(super) struct FakeSummarizer {
    calls: AtomicUsize,
    responses: Mutex<VecDeque<Result<String, SummarizerError>>>,
}

impl FakeSummarizer {
    pub(super) fn with_responses(responses: Vec<Result<String, SummarizerError>>) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            responses: Mutex::new(responses.into()),
        }
    }

    pub(super) fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ContextSummarizer for FakeSummarizer {
    async fn summarize(&self, _history: &[String]) -> Result<String, SummarizerError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Ok(String::new()))
    }
}

pub(super) struct PanicSummarizer {
    pub(super) calls: AtomicUsize,
}

#[async_trait]
impl ContextSummarizer for PanicSummarizer {
    async fn summarize(&self, _history: &[String]) -> Result<String, SummarizerError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        panic!("summarizer fell over");
    }
}

pub(super) fn ok(text: &str) -> Result<String, SummarizerError> {
    Ok(text.to_string())
}

pub(super) fn failed() -> Result<String, SummarizerError> {
    Err(SummarizerError::Failed(crate::llm::LlmError::ApiError(
        "boom".to_string(),
    )))
}

pub(super) fn unavailable() -> Result<String, SummarizerError> {
    Err(SummarizerError::Unavailable)
}

pub(super) struct Harness {
    pub(super) scheduler: ContextCompressionScheduler,
    pub(super) manager: Arc<TranslationManager>,
    pub(super) generation: Arc<AtomicU64>,
    pub(super) tx: watch::Sender<bool>,
    pub(super) rx: watch::Receiver<bool>,
}

pub(super) fn harness(
    manager: Arc<TranslationManager>,
    summarizer: Arc<dyn ContextSummarizer>,
    cooldown_ms: u64,
) -> Harness {
    let (tx, rx) = watch::channel(false);
    let generation = Arc::new(AtomicU64::new(0));
    let scheduler = ContextCompressionScheduler::new(
        Arc::clone(&manager),
        summarizer,
        Arc::clone(&generation),
        cooldown_ms,
    );
    Harness {
        scheduler,
        manager,
        generation,
        tx,
        rx,
    }
}

pub(super) async fn advance(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

pub(super) async fn settle() {
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
}
