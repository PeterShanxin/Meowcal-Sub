//! Serial inference and FIFO presentation, independent of the capture loop.

use crate::event_payloads::TranslationPayload;
use crate::llm::{BackendId, TranslationDisplayState, TranslationManager, TranslationOutcome};
use crate::pipeline_deadline::{await_within_deadline, SlotOutcome, TRANSLATION_DEADLINE};
use crate::pipeline_session::PipelineClock;
pub use crate::subtitle_queue::Frame;
use crate::subtitle_queue::{Completion, Job, Presentation, SubtitleQueue};
use crate::sync_utils::lock_or_recover;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{watch, Notify};
use tokio::task::JoinHandle;
use tracing::{info, warn};

mod presentation;

struct TranslationTask(JoinHandle<Option<TranslationPayload>>);

impl Drop for TranslationTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub struct TranslationWorkers {
    shutdown: watch::Sender<bool>,
    handles: Vec<JoinHandle<()>>,
}

impl TranslationWorkers {
    pub async fn stop(self) {
        self.shutdown.send_replace(true);
        for handle in self.handles {
            if let Err(error) = handle.await {
                warn!(%error, "Subtitle worker failed");
            }
        }
    }
}

#[derive(Clone)]
pub struct Translator {
    app: AppHandle,
    manager: Arc<TranslationManager>,
    clock: Arc<PipelineClock>,
    session_id: u64,
    source_language: String,
    target_language: String,
    queue: Arc<Mutex<SubtitleQueue<TranslationPayload>>>,
    changed: Arc<Notify>,
    generation: watch::Sender<u64>,
}

impl Translator {
    pub fn new(
        app: AppHandle,
        manager: Arc<TranslationManager>,
        clock: Arc<PipelineClock>,
        session_id: u64,
        source_language: String,
        target_language: String,
        interval: Duration,
    ) -> Self {
        let (generation, _) = watch::channel(0);
        Self {
            app,
            manager,
            clock,
            session_id,
            source_language,
            target_language,
            queue: Arc::new(Mutex::new(SubtitleQueue::new(interval))),
            changed: Arc::new(Notify::new()),
            generation,
        }
    }

    pub fn start(&self, stop_rx: watch::Receiver<bool>) -> TranslationWorkers {
        let (shutdown, shutdown_rx) = watch::channel(false);
        let inference = self.clone();
        let display = self.clone();
        let inference_stop = stop_rx.clone();
        let inference_shutdown = shutdown_rx.clone();
        TranslationWorkers {
            shutdown,
            handles: vec![
                tokio::spawn(async move {
                    inference
                        .infer_loop(inference_stop, inference_shutdown)
                        .await
                }),
                tokio::spawn(async move { display.display_loop(stop_rx, shutdown_rx).await }),
            ],
        }
    }

    pub fn observe(&self, frame: Frame, eligible: bool) -> bool {
        if !self.clock.is_translation_current(frame.region_generation) {
            return false;
        }
        let mut queue = lock_or_recover(&self.queue);
        let before = queue.stats(Instant::now());
        let capture_id = frame.token.capture_id;
        let source_chars = frame.text.chars().count();
        let admitted = queue.observe(frame, eligible, Instant::now());
        self.generation.send_if_modified(|value| {
            if *value == queue.generation() {
                return false;
            }
            *value = queue.generation();
            true
        });
        let after = queue.stats(Instant::now());
        if let Some(cue_id) = admitted {
            info!(
                session_id = self.session_id,
                cue_id,
                capture_id,
                source_chars,
                depth = after.depth,
                "pipeline_cue_enqueued"
            );
            self.changed.notify_one();
        }
        if after.skipped != before.skipped {
            warn!(
                session_id = self.session_id,
                skipped = after.skipped - before.skipped,
                depth = after.depth,
                "Subtitle queue recovered from a severe stall"
            );
        }
        admitted.is_some()
    }

    pub fn reset(&self) {
        let mut queue = lock_or_recover(&self.queue);
        queue.reset();
        self.generation.send_replace(queue.generation());
        self.changed.notify_one();
    }

    pub fn is_busy(&self) -> bool {
        lock_or_recover(&self.queue).is_busy()
    }

    /// Notices cannot race a newly displayed line or spend a queued cue's
    /// ordering position. All display events receive their own sequence.
    pub(crate) fn emit_if_idle(&self, mut payload: TranslationPayload) {
        let queue = lock_or_recover(&self.queue);
        if queue.is_busy() || !self.clock.is_session_current(self.session_id) {
            return;
        }
        payload.update_id = self.clock.next_update();
        if let Err(error) = self.app.emit("translation-update", payload) {
            warn!(%error, "Failed to emit subtitle notice");
        }
    }

    async fn infer_loop(
        &self,
        mut stop: watch::Receiver<bool>,
        mut shutdown: watch::Receiver<bool>,
    ) {
        let mut generation = self.generation.subscribe();
        loop {
            if *stop.borrow()
                || *shutdown.borrow()
                || !self.clock.is_session_current(self.session_id)
            {
                break;
            }
            let job = {
                let mut queue = lock_or_recover(&self.queue);
                let before = queue.stats(Instant::now());
                let job = queue.next_job(Instant::now());
                let after = queue.stats(Instant::now());
                if after.skipped > before.skipped {
                    warn!(
                        session_id = self.session_id,
                        skipped = after.skipped - before.skipped,
                        oldest_ms = before.oldest_ms,
                        depth = after.depth,
                        "Subtitle queue recovered from a severe stall"
                    );
                }
                self.generation.send_if_modified(|value| {
                    if *value == queue.generation() {
                        return false;
                    }
                    *value = queue.generation();
                    true
                });
                job
            };
            let Some(job) = job else {
                tokio::select! {
                    biased;
                    _ = stop.changed() => break,
                    _ = shutdown.changed() => break,
                    _ = self.changed.notified() => continue,
                }
            };
            if *generation.borrow_and_update() != job.generation
                || !self
                    .clock
                    .is_translation_current(job.frame.region_generation)
            {
                continue;
            }
            let worker = self.clone();
            let task_job = job.clone();
            let task_stop = stop.clone();
            let mut task = TranslationTask(tokio::spawn(async move {
                worker.translate(&task_job, task_stop).await
            }));
            let payload = tokio::select! {
                biased;
                _ = stop.changed() => break,
                _ = shutdown.changed() => break,
                _ = generation.changed() => continue,
                outcome = &mut task.0 => match outcome {
                    Ok(payload) => payload,
                    Err(error) => {
                        warn!(%error, cue_id = job.cue_id, "Subtitle translation task failed");
                        let mut payload = TranslationPayload::engine_slow(self.session_id, job.frame.token.capture_id, 0, job.frame.started.elapsed().as_millis() as u64);
                        payload.display_state = TranslationDisplayState::TemporarilyUnavailable;
                        payload.warnings.push("translation_task_failed".to_string());
                        Some(payload)
                    }
                },
            };
            let Some(payload) = payload else {
                continue;
            };
            let presentation = match payload.display_state {
                TranslationDisplayState::Translated => Presentation::Translated,
                TranslationDisplayState::SourceOnly
                | TranslationDisplayState::Stopped
                | TranslationDisplayState::NoSubtitleText => Presentation::Clear,
                _ => Presentation::Preserve,
            };
            let retryable = crate::llm::retryable_cue(payload.display_state, &payload.warnings);
            let text = payload.translated.clone();
            lock_or_recover(&self.queue).complete(
                &job,
                Completion {
                    result: payload,
                    text,
                    presentation,
                    retryable,
                },
                Instant::now(),
            );
        }
    }

    fn engine_is_managed(&self) -> bool {
        self.app
            .try_state::<crate::app_state::AppState>()
            .is_some_and(|state| {
                lock_or_recover(&state.config)
                    .translation
                    .foundry_local
                    .managed_runtime
                    .is_some()
            })
    }

    async fn translate(
        &self,
        job: &Job,
        mut stop: watch::Receiver<bool>,
    ) -> Option<TranslationPayload> {
        let model_started = Instant::now();
        let translation = self.manager.translate_with_context(
            &job.frame.text,
            &self.source_language,
            &self.target_language,
            job.frame.context_prompt.as_deref(),
        );
        let outcome =
            match await_within_deadline(translation, TRANSLATION_DEADLINE, &mut stop).await {
                SlotOutcome::Finished(outcome) => outcome,
                SlotOutcome::Cancelled => return None,
                SlotOutcome::DeadlineMissed => {
                    let waited_ms = TRANSLATION_DEADLINE.as_millis() as u64;
                    self.manager.record_abandoned(waited_ms as u128);
                    warn!(
                        session_id = self.session_id,
                        cue_id = job.cue_id,
                        waited_ms,
                        "Translation missed its deadline"
                    );
                    return Some(TranslationPayload::engine_slow(
                        self.session_id,
                        job.frame.token.capture_id,
                        waited_ms,
                        job.frame.started.elapsed().as_millis() as u64,
                    ));
                }
            };
        let TranslationOutcome {
            translated,
            backend_used,
            warnings,
            display_state,
        } = outcome;
        if engine_needs_restart(self.engine_is_managed(), &warnings) {
            crate::core_client::recover_transport();
        }
        Some(TranslationPayload {
            session_id: self.session_id,
            capture_id: job.frame.token.capture_id,
            cue_id: job.cue_id,
            update_id: 0,
            original: job.frame.text.clone(),
            translated,
            backend_used: backend_used.as_str().to_string(),
            warnings,
            display_state,
            timestamp: timestamp(),
            model_ms: model_started.elapsed().as_millis() as u64,
            total_ms: job.frame.started.elapsed().as_millis() as u64,
        })
    }
}

fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn engine_needs_restart(managed: bool, warnings: &[String]) -> bool {
    let warned = |code| warnings.contains(&format!("{}: {code}", BackendId::FoundryLocal.as_str()));
    managed && (warned("not_ready") || warned("engine_recovery_failed"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stopped_managed_engines_recover_but_active_recovery_is_not_restarted() {
        for code in ["not_ready", "engine_recovery_failed", "engine_recovering"] {
            let warnings = vec![format!("local_engine: {code}")];
            assert_eq!(
                engine_needs_restart(true, &warnings),
                code != "engine_recovering"
            );
            assert!(!engine_needs_restart(false, &warnings));
        }
    }
}
