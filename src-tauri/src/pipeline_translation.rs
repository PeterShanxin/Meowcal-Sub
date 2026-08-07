// =============================================================================
// PIPELINE_TRANSLATION.RS - Translating a line without stopping the camera
// =============================================================================
// The capture loop used to await the model inline. For the length of every
// translation - 406ms at the median, 944ms at p90, and once 27.6 seconds - it
// took no frames at all, so a subtitle that appeared during a translation was
// not noticed until the previous one finished. That put up to a whole model
// call on the worst-case latency of the *next* line, which is the part a viewer
// actually feels.
//
// Translation runs here instead, on its own task, while the loop keeps
// capturing. Two rules keep that honest:
//
// - One at a time. The local model serialises anyway, so letting frames pile
//   into it would multiply latency rather than hide it. While a translation is
//   in flight the loop still captures and reads, it simply does not start a
//   second one - and because it leaves `last_text` alone when it declines, the
//   next read of the same subtitle tries again the moment the slot frees.
// - A result is shown only if it is still wanted. Newer captures do not make a
//   translation stale, but a newer translation, a moved region, or a new
//   session all do. See `PipelineClock::begin_translation`.
// =============================================================================

use crate::event_payloads::TranslationPayload;
use crate::ipc::{IpcMessage, SubtitleUpdatePayload};
use crate::llm::{BackendId, TranslationManager, TranslationOutcome};
use crate::pipeline_deadline::{await_within_deadline, SlotOutcome, TRANSLATION_DEADLINE};
use crate::pipeline_session::{PipelineClock, PipelineToken};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

/// One frame's worth of work, handed over once the loop has decided the text is
/// worth translating.
pub struct Frame {
    pub token: PipelineToken,
    pub text: String,
    pub context_prompt: Option<String>,
    /// When the capture that produced this text began, so the reported total
    /// covers what the viewer waited for rather than what this task did.
    pub started: Instant,
    pub capture_ms: u64,
    pub ocr_ms: u64,
}

/// Releases the translation slot however the spawned task ends.
///
/// Clearing the flag with a store after the `await` looks equivalent and is
/// not: an unwind skips it, and a panic in a detached task is swallowed by the
/// runtime, so nothing would log and nothing would recover. The flag would stay
/// set for the rest of the session - `try_spawn` refusing every frame, and,
/// because `is_busy` also gates the loop's notices, the overlay frozen on the
/// last translated line with only a debug line to show for it. Stop and start
/// would be the only way back.
///
/// The panic is reachable: the translation manager locks its diagnostics with
/// `unwrap`, and this codebase treats a poisoned mutex as expected enough to
/// keep a `lock_or_recover` helper for it.
struct InFlightGuard(Arc<AtomicBool>);

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        // A swallowed panic is invisible otherwise, and a translation slot that
        // released itself for this reason is worth knowing about.
        if std::thread::panicking() {
            error!("Translation task panicked; releasing the translation slot");
        }
        self.0.store(false, Ordering::SeqCst);
    }
}

/// The handles a translation task needs, cloned once per session rather than
/// per frame.
#[derive(Clone)]
pub struct Translator {
    pub app: AppHandle,
    pub manager: Arc<TranslationManager>,
    pub clock: Arc<PipelineClock>,
    pub session_id: u64,
    pub source_language: String,
    pub target_language: String,
    /// Whether a translation is running. The loop reads it to decide whether to
    /// start another; `InFlightGuard` clears it however the task ends, so a
    /// discarded result cannot wedge the pipeline closed.
    in_flight: Arc<AtomicBool>,
    /// What the last completed translation came from. The duplicate filter
    /// consults it, and it is written from the task, so it cannot stay a local.
    last_backend_was_mock: Arc<AtomicBool>,
}

impl Translator {
    pub fn new(
        app: AppHandle,
        manager: Arc<TranslationManager>,
        clock: Arc<PipelineClock>,
        session_id: u64,
        source_language: String,
        target_language: String,
    ) -> Self {
        Self {
            app,
            manager,
            clock,
            session_id,
            source_language,
            target_language,
            in_flight: Arc::new(AtomicBool::new(false)),
            // Nothing has translated yet, and the mock-retry cooldown must not
            // fire before a real backend has been heard from.
            last_backend_was_mock: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Whether the last completed translation fell through to the mock backend.
    pub fn last_backend_was_mock(&self) -> bool {
        self.last_backend_was_mock.load(Ordering::SeqCst)
    }

    pub fn set_last_backend_was_mock(&self, was_mock: bool) {
        self.last_backend_was_mock.store(was_mock, Ordering::SeqCst);
    }

    /// Whether a translation is running.
    ///
    /// The loop consults this before emitting a notice of its own. The frontend
    /// accepts an update only when its `capture_id` exceeds the last one shown,
    /// and the loop's captures run ahead of whatever the model is still working
    /// on - so a notice emitted mid-translation carries a higher id and would
    /// make the translation, when it lands, look stale and be dropped. Staying
    /// quiet until the slot frees keeps emissions in id order, and reads better
    /// besides: the viewer is not told the region is empty while the line they
    /// are looking at is still being translated.
    pub fn is_busy(&self) -> bool {
        self.in_flight.load(Ordering::SeqCst)
    }

    /// Start translating, unless one is already running.
    ///
    /// Returns whether the frame was taken. A refused frame is not an error:
    /// the loop keeps the previous `last_text`, so the next read of the same
    /// subtitle offers it again, and the freed slot picks it up one capture
    /// period later rather than a whole model call later.
    ///
    /// The loop keeps two things on its own side of this handover. `last_text`
    /// and the notice state are per-frame bookkeeping it alone can order. So is
    /// the context generation counter: the summarization scheduler reads it as
    /// the generation a run was scheduled at, and a bump that landed from in
    /// here would arrive after that read and make every scheduled run look like
    /// the text had changed underneath it and cancel itself.
    pub fn try_spawn(&self, frame: Frame, stop_rx: watch::Receiver<bool>) -> bool {
        if self
            .in_flight
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return false;
        }

        let worker = self.clone();
        let guard = InFlightGuard(Arc::clone(&self.in_flight));
        tokio::spawn(async move {
            let _guard = guard;
            worker.run(frame, stop_rx).await;
        });
        true
    }

    /// Report a line the engine did not finish in time, and give the slot back.
    ///
    /// Returning from `run` drops the translation future, which disconnects the
    /// HTTP request and stops the server generating, and drops `InFlightGuard`,
    /// which frees the slot for the next capture.
    ///
    /// The notice matters as much as the cancellation. Before this, a slow call
    /// produced silence - `is_busy` suppressed the loop's notices and no result
    /// ever arrived - so a busy machine and a broken engine looked identical to
    /// the viewer. Emitting the state under the frame's own `capture_id` keeps
    /// it in the ordering the frontend accepts.
    fn abandon_slow_translation(&self, frame: &Frame) {
        let waited_ms = TRANSLATION_DEADLINE.as_millis() as u64;
        warn!(
            session_id = self.session_id,
            capture_id = frame.token.capture_id,
            deadline_ms = waited_ms,
            "Abandoned translation that missed its deadline"
        );
        // Dropping the backend future skips every diagnostics write inside it,
        // so without this a chronically slow engine reports as perfectly healthy
        // while the overlay says the opposite.
        self.manager.record_abandoned(waited_ms as u128);
        if let Err(error) = self.app.emit(
            "translation-update",
            TranslationPayload::engine_slow(
                self.session_id,
                frame.token.capture_id,
                waited_ms,
                frame.started.elapsed().as_millis() as u64,
            ),
        ) {
            warn!("⚠️ Failed to emit engine-slow notice: {}", error);
        }
    }

    async fn run(&self, frame: Frame, mut stop_rx: watch::Receiver<bool>) {
        let translation_id = self.clock.begin_translation();

        let model_started = Instant::now();
        let translation = self.manager.translate_with_context(
            &frame.text,
            &self.source_language,
            &self.target_language,
            frame.context_prompt.as_deref(),
        );
        let outcome =
            match await_within_deadline(translation, TRANSLATION_DEADLINE, &mut stop_rx).await {
                SlotOutcome::Finished(outcome) => outcome,
                SlotOutcome::Cancelled => {
                    info!(
                        session_id = self.session_id,
                        capture_id = frame.token.capture_id,
                        "Cancelled in-flight translation"
                    );
                    return;
                }
                SlotOutcome::DeadlineMissed => {
                    self.abandon_slow_translation(&frame);
                    return;
                }
            };
        let model_ms = model_started.elapsed().as_millis() as u64;

        let TranslationOutcome {
            translated,
            backend_used,
            warnings,
            display_state,
        } = outcome;

        if *stop_rx.borrow()
            || !self.clock.is_session_current(self.session_id)
            || !self.clock.is_translation_current(translation_id)
        {
            info!(
                session_id = self.session_id,
                capture_id = frame.token.capture_id,
                "Discarding stale in-flight translation result"
            );
            return;
        }

        self.set_last_backend_was_mock(backend_used == BackendId::Mock);

        // The pair, so a bad line can be blamed on OCR or on the model without
        // reproducing the episode.
        debug!(source = %frame.text, translated = %translated, "Translated");

        let overlay_started = Instant::now();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let backend_str = backend_used.as_str().to_string();

        let payload = TranslationPayload {
            session_id: self.session_id,
            capture_id: frame.token.capture_id,
            original: frame.text.clone(),
            translated: translated.clone(),
            backend_used: backend_str.clone(),
            warnings,
            display_state,
            timestamp,
            model_ms,
            total_ms: frame.started.elapsed().as_millis() as u64,
        };

        if let Err(e) = self.app.emit("translation-update", payload) {
            warn!("⚠️ Failed to emit event: {}", e);
        }

        if display_state == crate::llm::TranslationDisplayState::Translated {
            let subtitle_payload = SubtitleUpdatePayload {
                text: translated,
                source_text: frame.text,
                timestamp: timestamp.to_string(),
                backend_used: Some(backend_str),
            };
            crate::commands::send_overlay_message(
                &self.app,
                IpcMessage::with_payload("Subtitle.Update", subtitle_payload),
            )
            .await;
        }

        info!(
            session_id = self.session_id,
            capture_id = frame.token.capture_id,
            capture_ms = frame.capture_ms,
            ocr_ms = frame.ocr_ms,
            model_ms,
            overlay_ms = overlay_started.elapsed().as_millis() as u64,
            total_ms = frame.started.elapsed().as_millis() as u64,
            "pipeline_frame_complete"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slot_is_released_when_the_task_finishes() {
        let flag = Arc::new(AtomicBool::new(true));
        drop(InFlightGuard(Arc::clone(&flag)));
        assert!(!flag.load(Ordering::SeqCst));
    }

    // The failure this guard exists for. A store placed after the await is
    // skipped by an unwind, tokio swallows a detached task's panic, and the
    // pipeline would then refuse every frame for the rest of the session with
    // nothing above debug to say why.
    #[test]
    fn the_slot_is_released_when_the_task_panics() {
        let flag = Arc::new(AtomicBool::new(true));
        let guarded = Arc::clone(&flag);

        let panicked = std::panic::catch_unwind(move || {
            let _guard = InFlightGuard(guarded);
            panic!("translation task fell over");
        })
        .is_err();

        assert!(
            panicked,
            "the fixture must actually panic to prove anything"
        );
        assert!(!flag.load(Ordering::SeqCst));
    }
}
