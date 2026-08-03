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
use crate::pipeline_session::{PipelineClock, PipelineToken};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::sync::watch;
use tracing::{debug, info, warn};

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
    /// start another; the task clears it on the way out, including on an early
    /// return, so a discarded result cannot wedge the pipeline closed.
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
        tokio::spawn(async move {
            worker.run(frame, stop_rx).await;
            worker.in_flight.store(false, Ordering::SeqCst);
        });
        true
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
        let outcome = tokio::select! {
            outcome = translation => outcome,
            changed = stop_rx.changed() => {
                if changed.is_ok() && *stop_rx.borrow() {
                    info!(
                        session_id = self.session_id,
                        capture_id = frame.token.capture_id,
                        "Cancelled in-flight translation"
                    );
                }
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
