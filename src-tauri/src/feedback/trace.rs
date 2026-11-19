//! Per-frame observations. The queue can finish an older cue while capture is already on a new one.
use crate::pipeline_session::PipelineToken;
use crate::sync_utils::lock_or_recover;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Stage {
    pub(super) stage: &'static str,
    pub(super) status: &'static str,
    pub(super) elapsed_ms: Option<u64>,
    pub(super) code: Option<&'static str>,
}

#[derive(Clone)]
pub(super) struct Frame {
    pub(super) token: PipelineToken,
    pub(super) verbose: bool,
    pub(super) stages: Vec<Stage>,
    pub(super) image: Option<Arc<[u8]>>,
    pub(super) recognized: Option<String>,
    pub(super) translation: Option<String>,
    changed: Instant,
}

impl Frame {
    fn new(token: PipelineToken) -> Self {
        Self {
            token,
            verbose: crate::logging_settings::verbose(),
            stages: Vec::new(),
            image: None,
            recognized: None,
            translation: None,
            changed: Instant::now(),
        }
    }

    fn observe(&mut self, stage: Stage) {
        if let Some(previous) = self.stages.iter_mut().find(|s| s.stage == stage.stage) {
            // A fast WebView can acknowledge delivery before the emitter returns.
            if !(previous.stage == "overlay" && previous.status == "ok" && stage.status == "sent") {
                *previous = stage;
            }
        } else {
            self.stages.push(stage);
        }
        self.changed = Instant::now();
    }

    pub(super) fn application_error(stage: &'static str, code: &'static str) -> Self {
        let mut frame = Self::new(PipelineToken {
            session_id: 0,
            capture_id: 0,
        });
        frame.observe(Stage {
            stage,
            status: "failed",
            code: Some(code),
            elapsed_ms: None,
        });
        frame
    }
}

#[derive(Default)]
struct History {
    session: Option<u64>,
    frames: VecDeque<Frame>,
    fault: Option<Frame>,
    completed: Option<Frame>,
}

impl History {
    fn frame(&mut self, token: PipelineToken) -> Option<&mut Frame> {
        if self.session.is_some_and(|id| id > token.session_id) {
            return None;
        }
        if self.session != Some(token.session_id) {
            *self = Self {
                session: Some(token.session_id),
                ..Self::default()
            };
        }
        if let Some(index) = self.frames.iter().position(|f| f.token == token) {
            return self.frames.get_mut(index);
        }
        if self.frames.len() == 16 {
            // The one inference worker can outlive many capture intervals.
            let oldest = self
                .frames
                .iter()
                .position(|f| {
                    !f.stages
                        .iter()
                        .any(|s| s.stage == "translation" && s.status == "running")
                })
                .unwrap_or(0);
            self.frames.remove(oldest);
        }
        self.frames.push_back(Frame::new(token));
        self.frames.back_mut()
    }

    fn select(&self, fault: Option<&str>) -> Option<Frame> {
        if let Some(code) = fault {
            if let Some(frame) = self
                .fault
                .as_ref()
                .filter(|f| f.stages.iter().any(|s| s.code == Some(code)))
            {
                return Some(frame.clone());
            }
            return None;
        }
        let last = self.frames.back()?;
        if let Some(inflight) = self.frames.iter().rev().find(|f| {
            f.stages
                .iter()
                .any(|s| s.stage == "translation" && s.status == "running")
        }) {
            // New captures continue even while the single model worker is blocked.
            if self.completed.is_none() || inflight.changed.elapsed() >= Duration::from_secs(5) {
                let mut snapshot = inflight.clone();
                if let Some(stage) = snapshot
                    .stages
                    .iter_mut()
                    .find(|s| s.stage == "translation")
                {
                    stage.elapsed_ms = Some(inflight.changed.elapsed().as_millis() as u64);
                }
                return Some(snapshot);
            }
        }
        // Prefer the last informative cue over a new capture still in flight.
        // A sustained quiet region eventually supersedes that cue.
        self.fault
            .iter()
            .chain(self.completed.iter())
            .filter(|f| last.changed.saturating_duration_since(f.changed) < Duration::from_secs(5))
            .max_by_key(|f| f.changed)
            .or(Some(last))
            .cloned()
    }
}

static HISTORY: LazyLock<Mutex<History>> = LazyLock::new(Mutex::default);

pub(crate) fn begin_session(id: u64) {
    let mut history = lock_or_recover(&HISTORY);
    if history.session.is_none_or(|old| old < id) {
        *history = History {
            session: Some(id),
            ..History::default()
        };
    }
}

pub(crate) fn stop_recording() {
    for frame in &mut lock_or_recover(&HISTORY).frames {
        for stage in frame.stages.iter_mut().filter(|s| s.status == "running") {
            stage.status = "skipped";
            stage.code = Some("LOGGING_STOPPED");
        }
    }
}

pub(super) fn select(code: Option<&str>, reference: Option<&str>) -> Option<Frame> {
    let history = lock_or_recover(&HISTORY);
    if let Some(id) = reference {
        let (session, capture) = id.split_once('/')?;
        let token = PipelineToken {
            session_id: session.parse().ok()?,
            capture_id: capture.parse().ok()?,
        };
        return history
            .frames
            .iter()
            .chain(history.fault.iter())
            .find(|f| {
                f.token == token
                    && f.stages
                        .iter()
                        .any(|stage| stage.code == code && stage.status == "failed")
            })
            .cloned();
    }
    history.select(code)
}

pub(crate) fn observe(
    token: PipelineToken,
    stage: &'static str,
    status: &'static str,
    ms: Option<u64>,
    code: Option<&'static str>,
) {
    if !crate::logging_settings::enabled(&tracing::Level::INFO) {
        return;
    }
    let mut history = lock_or_recover(&HISTORY);
    if !crate::logging_settings::enabled(&tracing::Level::INFO) {
        return;
    }
    let Some(frame) = history.frame(token) else {
        return;
    };
    frame.observe(Stage {
        stage,
        status,
        elapsed_ms: ms,
        code,
    });
    if status == "failed" {
        tracing::warn!(
            session_id = token.session_id,
            capture_id = token.capture_id,
            stage,
            code,
            "pipeline_feedback_fault"
        );
        history.fault = Some(frame.clone());
    } else if stage == "overlay" && matches!(status, "ok" | "sent") {
        history.completed = Some(frame.clone());
    }
}

pub(crate) struct Inference(Option<PipelineToken>);

impl Inference {
    pub(crate) fn finished(mut self) {
        self.0 = None;
    }
}

impl Drop for Inference {
    fn drop(&mut self) {
        if let Some(token) = self.0 {
            observe(
                token,
                "translation",
                "skipped",
                None,
                Some("TRANSLATION_CANCELLED"),
            );
        }
    }
}

pub(crate) fn inference(frame: &crate::subtitle_queue::Frame) -> Inference {
    // A queued cue may have aged past the frame ring before inference starts.
    // Both timings here belong to the same admitted capture.
    observe(frame.token, "capture", "ok", Some(frame.capture_ms), None);
    observe(frame.token, "ocr", "ok", Some(frame.ocr_ms), None);
    observe(frame.token, "translation", "running", None, None);
    Inference(Some(frame.token))
}

pub(crate) fn overlay_received(token: PipelineToken) {
    if !crate::logging_settings::enabled(&tracing::Level::INFO) {
        return;
    }
    let mut history = lock_or_recover(&HISTORY);
    let Some(frame) = history
        .frames
        .iter_mut()
        .find(|f| f.token == token && f.stages.iter().any(|s| s.stage == "overlay"))
    else {
        return;
    };
    frame.observe(Stage {
        stage: "overlay",
        status: "ok",
        elapsed_ms: None,
        code: None,
    });
    history.completed = Some(frame.clone());
}

pub(crate) fn screenshot(token: PipelineToken, png: &[u8]) {
    // 16 frames at most, plus pinned previews. Large captures stay local on disk.
    if png.len() <= 2 * 1024 * 1024 && crate::logging_settings::verbose() {
        if let Some(frame) = lock_or_recover(&HISTORY).frame(token) {
            frame.image = Some(Arc::from(png));
        }
    }
}

pub(crate) fn ocr_text(token: PipelineToken, text: &str) {
    if !text.is_empty() && text.len() <= 32 * 1024 && crate::logging_settings::verbose() {
        if let Some(frame) = lock_or_recover(&HISTORY).frame(token) {
            frame.recognized = Some(text.to_owned());
        }
    }
}

pub(crate) fn translated_text(token: PipelineToken, source: &str, translated: &str) {
    if !translated.is_empty()
        && source.len() + translated.len() <= 64 * 1024
        && crate::logging_settings::verbose()
    {
        if let Some(frame) = lock_or_recover(&HISTORY).frame(token) {
            frame.translation = Some(format!("{source}\n{translated}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frames_never_mix_across_sessions_and_errors_are_pinned() {
        let mut history = History::default();
        let a = PipelineToken {
            session_id: 3,
            capture_id: 10,
        };
        let frame = history.frame(a).unwrap();
        frame.observe(Stage {
            stage: "ocr",
            status: "failed",
            elapsed_ms: Some(33),
            code: Some("OCR_FAILED"),
        });
        history.fault = Some(frame.clone());
        for capture_id in 11..60 {
            history
                .frame(PipelineToken {
                    session_id: 3,
                    capture_id,
                })
                .unwrap();
        }
        assert_eq!(history.select(Some("OCR_FAILED")).unwrap().token, a);
        assert!(history.select(Some("CAPTURE_FAILED")).is_none());
        history
            .frame(PipelineToken {
                session_id: 4,
                capture_id: 1,
            })
            .unwrap();
        assert!(history.frame(a).is_none());
        assert!(history.select(Some("OCR_FAILED")).is_none());
    }
}
