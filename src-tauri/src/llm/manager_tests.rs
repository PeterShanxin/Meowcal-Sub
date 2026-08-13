use super::*;
use crate::config::{ContextLevel, TranslationConfig};
use crate::llm::LlmError;
use async_trait::async_trait;
use std::sync::atomic::AtomicUsize;

// Fixtures shared with `tier_tests` (same module tree, same cfg(test) build).
pub(super) struct TestBackend {
    pub(super) id: BackendId,
    pub(super) available: bool,
    pub(super) ready_state: ReadyState,
    pub(super) response: Result<String, LlmError>,
    pub(super) delay_ms: u64,
}

pub(super) struct CountingBackend {
    calls: Arc<AtomicUsize>,
    pub(super) response: Result<String, LlmError>,
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

pub(super) fn base_config() -> TranslationConfig {
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

// Wave-3 characterization: the context tier degrades on timeout, in the
// Full -> MemoryOnly -> None order, and the chain still reaches the fallback.
#[tokio::test(start_paused = true)]
async fn context_tier_degrades_on_timeout_and_still_falls_back() {
    let mut config = base_config();
    config.foundry_local.timeout_ms = 100;
    let backends: Vec<Box<dyn TranslatorBackend>> = vec![
        Box::new(TestBackend {
            id: BackendId::FoundryLocal,
            available: true,
            ready_state: ReadyState::Ready,
            response: Ok("slow".to_string()),
            delay_ms: 60_000,
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
    let manager = TranslationManager::with_backends(config, backends, diagnostics, 500);

    let outcome = manager
        .translate_with_context("你好", "zh-CN", "en-US", Some("session context"))
        .await;

    assert_eq!(outcome.backend_used, BackendId::Mock);
    assert_eq!(
        outcome.display_state,
        TranslationDisplayState::TemporarilyUnavailable
    );
    assert!(
        outcome
            .warnings
            .iter()
            .any(|warning| warning == "local_engine: context_degraded"),
        "the Full tier must degrade once the attempt times out: {:?}",
        outcome.warnings
    );
    assert!(
        outcome
            .warnings
            .iter()
            .any(|warning| warning == "local_engine: timeout"),
        "the timeout must reach the warnings: {:?}",
        outcome.warnings
    );
}

// Wave-3 characterization: a slow-but-successful answer degrades the stored
// tier for future requests, and the line still displays as translated.
#[tokio::test]
async fn context_tier_degrades_on_slow_success() {
    let mut config = base_config();
    config.foundry_local.timeout_ms = 8_000;
    let backends: Vec<Box<dyn TranslatorBackend>> = vec![Box::new(TestBackend {
        id: BackendId::FoundryLocal,
        available: true,
        ready_state: ReadyState::Ready,
        response: Ok("hello world".to_string()),
        delay_ms: 1900,
    })];

    let diagnostics = Arc::new(Mutex::new(TranslationDiagnosticsState::default()));
    let manager = TranslationManager::with_backends(config, backends, diagnostics, 500);

    let outcome = manager
        .translate_with_context("你好", "zh-CN", "en-US", Some("session context"))
        .await;

    assert_eq!(outcome.backend_used, BackendId::FoundryLocal);
    assert_eq!(outcome.display_state, TranslationDisplayState::Translated);
    assert!(
        outcome
            .warnings
            .iter()
            .any(|warning| warning == "local_engine: context_degraded_slow"),
        "a success slower than CONTEXT_SLOW_DEGRADE_MS must degrade the tier: {:?}",
        outcome.warnings
    );
}

#[test]
fn the_whole_fallback_chain_fits_inside_the_pipeline_deadline() {
    // Walk the arithmetic the retry loop actually performs. Asserting only that
    // the attempt cap is smaller than the deadline was not enough: at 4000ms
    // against a 5000ms deadline the first attempt fit, the retry started, and
    // the line was abandoned 1000ms into it - so the passthrough that would have
    // shown the untranslated source line was still unreachable.
    let deadline = crate::pipeline_deadline::TRANSLATION_DEADLINE;
    let budget = backend_budget();
    assert!(
        budget < deadline,
        "the budget must leave room for the passthrough"
    );

    let cap = Duration::from_millis(UNCONTEXTED_ATTEMPT_TIMEOUT_MS);
    let retry_delay = Duration::from_millis(FOUNDRY_TRANSIENT_RETRY_DELAY_MS);
    let mut elapsed = Duration::ZERO;
    let mut attempts = 0;

    // The loop retries while the budget still has more than one retry delay left.
    loop {
        let remaining = budget.saturating_sub(elapsed);
        elapsed += remaining.min(cap);
        attempts += 1;
        if attempts >= 1 + FOUNDRY_TRANSIENT_MAX_RETRIES
            || budget.saturating_sub(elapsed) <= retry_delay
        {
            break;
        }
    }

    assert!(
        attempts >= 2,
        "a stall clears on retry, so at least one retry must fit; got {attempts}"
    );
    assert!(
        elapsed < deadline,
        "the engine gave up at {elapsed:?}, at or past the {deadline:?} deadline,          so the passthrough never runs"
    );
}
