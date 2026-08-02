use crate::llm::TranslationDisplayState;
use crate::ocr_gate::OcrRejection;
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

/// Consecutive empty OCR frames before the overlay retires the current line.
///
/// One empty frame is normal between two subtitles; clearing on the first would
/// make every subtitle change flicker.
pub(crate) const EMPTY_OCR_CLEAR_FRAMES: u32 = 3;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TranslationPayload {
    pub session_id: u64,
    pub capture_id: u64,
    pub original: String,
    pub translated: String,
    pub backend_used: String,
    pub warnings: Vec<String>,
    pub display_state: TranslationDisplayState,
    pub timestamp: u64,
    /// Milliseconds the translation backend took for this line.
    pub model_ms: u64,
    /// Milliseconds from starting the capture to emitting this line.
    ///
    /// Surfaced so the overlay can show where the wait actually goes rather
    /// than leaving latency to guesswork.
    pub total_ms: u64,
}

impl TranslationPayload {
    /// The pipeline is running but the capture region holds no text.
    pub(crate) fn no_subtitle_text(session_id: u64, capture_id: u64) -> Self {
        Self::lifecycle(
            session_id,
            capture_id,
            TranslationDisplayState::NoSubtitleText,
        )
    }

    /// OCR read the region but the line was not reliable enough to translate.
    pub(crate) fn source_unreadable(
        session_id: u64,
        capture_id: u64,
        rejection: OcrRejection,
    ) -> Self {
        let mut payload = Self::lifecycle(
            session_id,
            capture_id,
            TranslationDisplayState::SourceUnreadable,
        );
        payload.warnings.push(rejection.as_str().to_string());
        payload
    }

    /// The pipeline has stopped; the overlay clears everything.
    pub(crate) fn stopped(session_id: u64, capture_id: u64) -> Self {
        Self::lifecycle(session_id, capture_id, TranslationDisplayState::Stopped)
    }

    fn lifecycle(session_id: u64, capture_id: u64, display_state: TranslationDisplayState) -> Self {
        Self {
            session_id,
            capture_id,
            original: String::new(),
            translated: String::new(),
            // No backend ran for a lifecycle notice. Naming one here made the
            // overlay diagnostics report "mock (source only)" during a perfectly
            // healthy Foundry session, which reads as a broken engine.
            backend_used: String::new(),
            warnings: Vec::new(),
            display_state,
            timestamp: now_ms(),
            model_ms: 0,
            total_ms: 0,
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CaptureStatusPayload {
    pub using_fallback: bool,
    pub message: String,
    pub is_error: bool,
}
