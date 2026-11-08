//! Native session activity reports and terminal capture-task cleanup.

use crate::app_state::AppState;
use crate::capture;
use crate::event_payloads::{
    CaptureStatusPayload, SubtitleActivityPayload, TranslationInterruptedPayload,
};
use crate::pipeline_session::PipelineToken;
use crate::subtitle_activity::{ActivityState, SubtitleActivity};
use crate::sync_utils::lock_or_recover;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::watch;
use tracing::info;

pub(crate) struct SessionStatus {
    app: AppHandle,
    session_id: u64,
    stopped: watch::Receiver<bool>,
    activity: SubtitleActivity,
}

impl SessionStatus {
    pub(crate) fn new(
        app: AppHandle,
        session_id: u64,
        stopped: watch::Receiver<bool>,
        interval: Duration,
    ) -> Self {
        let _ = app.emit(
            "subtitle-activity",
            SubtitleActivityPayload::new(
                PipelineToken {
                    session_id,
                    capture_id: 0,
                },
                ActivityState::Active,
            ),
        );
        Self {
            app,
            session_id,
            stopped,
            activity: SubtitleActivity::new(interval),
        }
    }

    pub(crate) fn observe(&mut self, token: PipelineToken, has_text: bool) {
        if let Some(state) = self.activity.observe(has_text, Instant::now()) {
            info!(
                session_id = token.session_id,
                capture_id = token.capture_id,
                state = state.as_str(),
                "subtitle_activity_changed"
            );
            let _ = self.app.emit(
                "subtitle-activity",
                SubtitleActivityPayload::new(token, state),
            );
        }
    }

    pub(crate) fn interrupt(&mut self) {
        self.activity.interrupt();
    }

    pub(crate) fn recognition_failed(&mut self) {
        if self.activity.recognition_failed() {
            let _ = self.app.emit("capture-status", CaptureStatusPayload {
                using_fallback: false,
                message: "Text recognition is not responding. Return to Home and restart translation.".to_string(),
                is_error: true,
            });
        }
    }
}

impl Drop for SessionStatus {
    fn drop(&mut self) {
        let state = self.app.state::<AppState>();
        let stopped = *self.stopped.borrow();
        let interrupted = state
            .pipeline_clock
            .interrupt_unexpected_exit(self.session_id, stopped);
        // A stale task must not close the replacement session's capture surface.
        if stopped || interrupted {
            capture::close_capture_session();
            *lock_or_recover(&state.is_running) = false;
            let _ = self.app.emit("display-context-changed", ());
        }
        if interrupted {
            let _ = self.app.emit(
                "translation-interrupted",
                TranslationInterruptedPayload {
                    session_id: self.session_id,
                },
            );
        }
        info!("Translation loop cleanup complete");
    }
}
