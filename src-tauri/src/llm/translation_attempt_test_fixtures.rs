use super::*;
use crate::llm::{
    BackendId, LlmError, PromptRouterOptions, ReadyState, TranslationDiagnosticsState,
    TranslatorBackend,
};
use crate::sync_utils::lock_or_recover;
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::time::Instant as VirtualInstant;

/// One scripted answer a fake backend gives, in order; the last step repeats.
#[derive(Clone)]
pub(super) enum ScriptedStep {
    Ok(String),
    Err(LlmError),
    /// Sleeps far beyond any attempt cap, so the runner's timeout fires.
    Hang,
}

pub(super) struct ScriptedBackend {
    id: BackendId,
    script: Vec<ScriptedStep>,
    pub(super) calls: Arc<AtomicUsize>,
    pub(super) virtual_call_times: Arc<Mutex<Vec<VirtualInstant>>>,
    pub(super) options_seen: Arc<Mutex<Vec<Option<PromptRouterOptions>>>>,
}

impl ScriptedBackend {
    pub(super) fn new(id: BackendId, script: Vec<ScriptedStep>) -> Self {
        Self {
            id,
            script,
            calls: Arc::new(AtomicUsize::new(0)),
            virtual_call_times: Arc::new(Mutex::new(Vec::new())),
            options_seen: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn step_for(&self, index: usize) -> ScriptedStep {
        self.script
            .get(index)
            .cloned()
            .or_else(|| self.script.last().cloned())
            .unwrap_or(ScriptedStep::Err(LlmError::ApiError(
                "no scripted step".to_string(),
            )))
    }
}

#[async_trait]
impl TranslatorBackend for ScriptedBackend {
    fn id(&self) -> BackendId {
        self.id
    }

    fn name(&self) -> &'static str {
        "Scripted"
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
        text: &str,
        source_language: &str,
        target_language: &str,
        context: Option<&str>,
        options: Option<PromptRouterOptions>,
    ) -> Result<String, LlmError> {
        let call_index = self.calls.fetch_add(1, Ordering::SeqCst);
        {
            let mut times = lock_or_recover(&self.virtual_call_times);
            times.push(VirtualInstant::now());
        }
        {
            let mut seen = lock_or_recover(&self.options_seen);
            seen.push(options);
        }
        let _ = (text, source_language, target_language, context);
        match self.step_for(call_index) {
            ScriptedStep::Ok(translated) => Ok(translated),
            ScriptedStep::Err(err) => Err(err),
            ScriptedStep::Hang => {
                tokio::time::sleep(Duration::from_secs(3_600)).await;
                Ok(String::new())
            }
        }
    }
}

pub(super) fn default_policy(max_attempts: usize) -> AttemptPolicy {
    AttemptPolicy {
        max_attempts,
        retry_delay_ms: 600,
        contexted_attempt_cap_ms: 2500,
        uncontexted_attempt_cap_ms: 2500,
        prompt_max_context_chars: 600,
        prompt_max_source_chars: 300,
    }
}

pub(super) fn budget(total_timeout_ms: u64) -> AttemptBudget {
    AttemptBudget {
        started: Instant::now(),
        total_timeout: Duration::from_millis(total_timeout_ms),
    }
}

pub(super) fn zh_request<'a>(
    text: &'a str,
    context: Option<&'a str>,
    context_used: bool,
) -> AttemptRequest<'a> {
    AttemptRequest {
        text,
        source_language: "zh-CN",
        target_language: "en-US",
        context_prompt: context,
        context_used,
    }
}

pub(super) fn expect_succeeded(outcome: AttemptOutcome) -> (String, bool) {
    match outcome {
        AttemptOutcome::Succeeded {
            translated,
            recovered_after_retry,
            ..
        } => (translated, recovered_after_retry),
        other => panic!("expected Succeeded, got {other:?}"),
    }
}

pub(super) fn expect_failed(outcome: AttemptOutcome) -> LlmError {
    match outcome {
        AttemptOutcome::Failed(err) => err,
        other => panic!("expected Failed, got {other:?}"),
    }
}

pub(super) fn expect_timed_out(outcome: AttemptOutcome) -> bool {
    match outcome {
        AttemptOutcome::TimedOut { total_exhausted } => total_exhausted,
        other => panic!("expected TimedOut, got {other:?}"),
    }
}

pub(super) fn harness(
    script: Vec<ScriptedStep>,
) -> (
    TranslationAttemptRunner,
    Arc<ScriptedBackend>,
    Arc<Mutex<TranslationDiagnosticsState>>,
) {
    let backend = Arc::new(ScriptedBackend::new(BackendId::FoundryLocal, script));
    let diagnostics = Arc::new(Mutex::new(TranslationDiagnosticsState::default()));
    let runner = TranslationAttemptRunner::new(default_policy(3), diagnostics.clone());
    (runner, backend, diagnostics)
}
