// Characterization for the context-tier progression state machine: the
// degradation order, the persisted effective tier, warning order, and prompt
// selection per tier. The exhausted/terminal paths live in the sibling
// `exhaustion_tests` module. Deterministic fakes only - no network, no real
// engine.

use super::*;
use crate::llm::translation_attempt::test_fixtures::{
    budget, default_policy, ScriptedBackend, ScriptedStep,
};
use crate::llm::{BackendId, LlmError, PromptRouterOptions, TranslationDiagnosticsState};
use crate::sync_utils::lock_or_recover;
use async_trait::async_trait;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::time::Duration;

// Fakes shared with the sibling `exhaustion_tests` module (same tree, same
// cfg(test) build).
pub(super) struct StepOutcome {
    pub(super) delay_ms: u64,
    pub(super) response: Result<String, LlmError>,
}

/// Backend that can sleep a per-call real delay and record the context prompt
/// and prompt options each call received.
pub(super) struct RecordingBackend {
    pub(super) script: Vec<StepOutcome>,
    pub(super) calls: AtomicUsize,
    pub(super) seen: Mutex<Vec<(Option<String>, Option<PromptRouterOptions>)>>,
}

impl RecordingBackend {
    fn step_for(&self, index: usize) -> &StepOutcome {
        &self.script[index.min(self.script.len().saturating_sub(1))]
    }
}

#[async_trait]
impl TranslatorBackend for RecordingBackend {
    fn id(&self) -> BackendId {
        BackendId::FoundryLocal
    }

    fn name(&self) -> &'static str {
        "Recording"
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
        text: &str,
        source_language: &str,
        target_language: &str,
    ) -> Result<String, LlmError> {
        self.translate_with_context_options(text, source_language, target_language, None, None)
            .await
    }

    async fn translate_with_context_options(
        &self,
        _text: &str,
        _source_language: &str,
        _target_language: &str,
        context: Option<&str>,
        options: Option<PromptRouterOptions>,
    ) -> Result<String, LlmError> {
        let index = self.calls.fetch_add(1, Ordering::SeqCst);
        {
            let mut seen = lock_or_recover(&self.seen);
            seen.push((context.map(str::to_string), options));
        }
        let step = self.step_for(index);
        if step.delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(step.delay_ms)).await;
        }
        step.response.clone()
    }
}

pub(super) fn tier_store(value: ContextTier) -> Arc<AtomicU8> {
    Arc::new(AtomicU8::new(value as u8))
}

pub(super) fn diagnostics() -> Arc<Mutex<TranslationDiagnosticsState>> {
    Arc::new(Mutex::new(TranslationDiagnosticsState::default()))
}

pub(super) fn plan<'a>(
    store: &'a AtomicU8,
    initial_tier: ContextTier,
    full: Option<&'a str>,
    memory: Option<&'a str>,
) -> TieredPlan<'a> {
    TieredPlan {
        text: "你好",
        source_language: "zh-CN",
        target_language: "en-US",
        full_context_prompt: full,
        memory_only_prompt: memory,
        initial_tier,
        tier_store: store,
    }
}

// The ordinary path: the first tier answers and the effective tier is stored
// unchanged - no degradation warnings, exactly one attempt with context.
#[tokio::test]
async fn first_tier_success_returns_translated_and_stores_the_tier() {
    let backend = Arc::new(ScriptedBackend::new(
        BackendId::FoundryLocal,
        vec![ScriptedStep::Ok("hello world".to_string())],
    ));
    let store = tier_store(ContextTier::Full);
    let planner = TranslationPlanner::new(default_policy(3), diagnostics());
    let mut warnings = Vec::new();

    let outcome = planner
        .run_tiered_sequence(
            backend.as_ref(),
            &plan(&store, ContextTier::Full, Some("FULL"), Some("MEM")),
            ReadyState::Ready,
            &budget(10_000),
            &mut warnings,
        )
        .await
        .expect("the first tier succeeds");

    assert_eq!(outcome.translated, "hello world");
    assert!(warnings.is_empty(), "no degradation: {:?}", warnings);
    assert_eq!(
        ContextTier::from_u8(store.load(Ordering::SeqCst)),
        ContextTier::Full,
        "the successful contexted tier is stored"
    );
    assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    let seen = lock_or_recover(&backend.options_seen);
    assert!(
        seen[0]
            .as_ref()
            .is_some_and(|options| options.enable_context),
        "the attempt must carry context options"
    );
}

// A timeout on the Full tier degrades to MemoryOnly, which then answers; the
// stored tier follows the degradation.
#[tokio::test(start_paused = true)]
async fn a_timeout_degrades_one_tier_and_the_next_tier_succeeds() {
    let backend = Arc::new(ScriptedBackend::new(
        BackendId::FoundryLocal,
        vec![
            ScriptedStep::Hang,
            ScriptedStep::Ok("hello world".to_string()),
        ],
    ));
    let store = tier_store(ContextTier::Full);
    let planner = TranslationPlanner::new(default_policy(3), diagnostics());
    let mut warnings = Vec::new();

    let outcome = planner
        .run_tiered_sequence(
            backend.as_ref(),
            &plan(&store, ContextTier::Full, Some("FULL"), Some("MEM")),
            ReadyState::Ready,
            &budget(10_000),
            &mut warnings,
        )
        .await
        .expect("the MemoryOnly tier succeeds");

    assert_eq!(outcome.translated, "hello world");
    assert_eq!(
        warnings,
        vec![
            "local_engine: timeout".to_string(),
            "local_engine: context_degraded".to_string(),
        ]
    );
    assert_eq!(
        ContextTier::from_u8(store.load(Ordering::SeqCst)),
        ContextTier::MemoryOnly,
        "the degraded tier is persisted for the next line"
    );
    assert_eq!(backend.calls.load(Ordering::SeqCst), 2);
}

// Only a tier whose context was actually used may be stored: with the memory
// prompt missing, MemoryOnly degrades to an unverified no-context run, and the
// stored tier must be left alone rather than overwritten.
#[tokio::test]
async fn an_unverified_tier_is_not_stored() {
    let backend = Arc::new(ScriptedBackend::new(
        BackendId::FoundryLocal,
        vec![ScriptedStep::Ok("hello world".to_string())],
    ));
    let store = tier_store(ContextTier::MemoryOnly);
    let planner = TranslationPlanner::new(default_policy(3), diagnostics());
    let mut warnings = Vec::new();

    let outcome = planner
        .run_tiered_sequence(
            backend.as_ref(),
            &plan(&store, ContextTier::MemoryOnly, None, None),
            ReadyState::Ready,
            &budget(10_000),
            &mut warnings,
        )
        .await
        .expect("the uncontexted run still succeeds");

    assert_eq!(outcome.translated, "hello world");
    assert!(warnings.is_empty(), "no degradation: {:?}", warnings);
    assert_eq!(
        ContextTier::from_u8(store.load(Ordering::SeqCst)),
        ContextTier::MemoryOnly,
        "the unverified tier must not be stored over the previous one"
    );
}

// A transient failure recovered on retry warns in the same order as today:
// the retry note before the slow-degradation note.
#[tokio::test]
async fn the_retry_warning_precedes_the_slow_degredation_warning() {
    let backend = RecordingBackend {
        script: vec![
            StepOutcome {
                delay_ms: 0,
                response: Err(LlmError::ApiError("connection refused".to_string())),
            },
            StepOutcome {
                delay_ms: 100,
                response: Ok("hello world".to_string()),
            },
        ],
        calls: AtomicUsize::new(0),
        seen: Mutex::new(Vec::new()),
    };
    let store = tier_store(ContextTier::Full);
    let planner = TranslationPlanner::with_slow_degrade_ms(default_policy(3), diagnostics(), 50);
    let mut warnings = Vec::new();

    let outcome = planner
        .run_tiered_sequence(
            &backend,
            &plan(&store, ContextTier::Full, Some("FULL"), Some("MEM")),
            ReadyState::Ready,
            &budget(10_000),
            &mut warnings,
        )
        .await
        .expect("the retried attempt succeeds");

    assert_eq!(outcome.translated, "hello world");
    assert_eq!(
        warnings,
        vec![
            "local_engine: recovered_after_retry".to_string(),
            "local_engine: context_degraded_slow".to_string(),
        ]
    );
    assert_eq!(
        ContextTier::from_u8(store.load(Ordering::SeqCst)),
        ContextTier::MemoryOnly
    );
}

// Each tier selects its own prompt: Full asks with the full context, and the
// degraded tier asks with the memory-only prompt.
#[tokio::test(start_paused = true)]
async fn each_tier_hands_its_own_prompt_to_the_backend() {
    let store = tier_store(ContextTier::Full);
    let planner = TranslationPlanner::new(default_policy(3), diagnostics());
    let mut warnings = Vec::new();

    let backend = Arc::new(ScriptedBackend::new(
        BackendId::FoundryLocal,
        vec![
            ScriptedStep::Hang,
            ScriptedStep::Ok("hello world".to_string()),
        ],
    ));
    let seen_contexts = Arc::new(Mutex::new(Vec::new()));
    let recorder = PromptRecorderBackend {
        inner: Arc::clone(&backend),
        seen: Arc::clone(&seen_contexts),
    };

    let outcome = planner
        .run_tiered_sequence(
            &recorder,
            &plan(&store, ContextTier::Full, Some("FULL"), Some("MEM")),
            ReadyState::Ready,
            &budget(10_000),
            &mut warnings,
        )
        .await
        .expect("the MemoryOnly tier succeeds");

    assert_eq!(outcome.translated, "hello world");
    let seen = lock_or_recover(&seen_contexts);
    assert_eq!(
        seen[0].as_deref(),
        Some("FULL"),
        "the Full tier asks with the full context"
    );
    assert_eq!(
        seen[1].as_deref(),
        Some("MEM"),
        "the degraded tier asks with the memory prompt"
    );
}

/// Delegates to a backend while recording the context prompt of each call.
struct PromptRecorderBackend {
    inner: Arc<ScriptedBackend>,
    seen: Arc<Mutex<Vec<Option<String>>>>,
}

#[async_trait]
impl TranslatorBackend for PromptRecorderBackend {
    fn id(&self) -> BackendId {
        self.inner.id()
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn is_available(&self) -> bool {
        self.inner.is_available()
    }

    fn ready_state(&self) -> ReadyState {
        self.inner.ready_state()
    }

    fn notes(&self) -> String {
        self.inner.notes()
    }

    async fn translate(
        &self,
        text: &str,
        source_language: &str,
        target_language: &str,
    ) -> Result<String, LlmError> {
        self.translate_with_context_options(text, source_language, target_language, None, None)
            .await
    }

    async fn translate_with_context_options(
        &self,
        text: &str,
        source_language: &str,
        target_language: &str,
        context: Option<&str>,
        options: Option<PromptRouterOptions>,
    ) -> Result<String, LlmError> {
        lock_or_recover(&self.seen).push(context.map(str::to_string));
        self.inner
            .translate_with_context_options(
                text,
                source_language,
                target_language,
                context,
                options,
            )
            .await
    }
}
