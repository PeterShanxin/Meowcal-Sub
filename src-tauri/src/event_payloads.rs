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

    /// Nothing to show for this frame, saying which kind of nothing it is.
    ///
    /// An empty region and one whose every line the band filter held need
    /// opposite responses from the viewer, and the pipeline reported both as
    /// "no text detected" - while a subtitle was on screen and 109 lines of it
    /// had just been discarded. See `BandFilter::held_lines` and issue #59.
    pub(crate) fn region_quiet(session_id: u64, capture_id: u64, held_lines: usize) -> Self {
        if held_lines > 0 {
            Self::source_unreadable(session_id, capture_id, OcrRejection::BandHeld)
        } else {
            Self::no_subtitle_text(session_id, capture_id)
        }
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

    /// The engine missed the deadline and the line was abandoned.
    ///
    /// `model_ms` carries how long the engine was given, so the overlay's
    /// diagnostics report the deadline that fired rather than a zero that reads
    /// as "no engine ran".
    ///
    /// `total_ms` is what the viewer waited, measured from the capture that
    /// produced the line - the same meaning it carries on a completed frame.
    /// Reporting the deadline in both under-stated it by the capture and OCR
    /// time, and made this payload's `total_ms` mean something different from
    /// every other one in the same readout.
    pub(crate) fn engine_slow(
        session_id: u64,
        capture_id: u64,
        waited_ms: u64,
        total_ms: u64,
    ) -> Self {
        let mut payload =
            Self::lifecycle(session_id, capture_id, TranslationDisplayState::EngineSlow);
        payload.model_ms = waited_ms;
        payload.total_ms = total_ms;
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

#[cfg(test)]
mod tests {
    use super::*;

    // The frontend switches on this string. `translation-display.js` lists the
    // states it knows and silently falls back to "translated" for anything else,
    // so a rename here would present an abandoned line as a real translation.
    #[test]
    fn the_engine_slow_state_serializes_under_the_name_the_frontend_matches() {
        let payload = TranslationPayload::engine_slow(1, 7, 5_000, 5_400);
        let json = serde_json::to_value(&payload).expect("payload should serialize");

        assert_eq!(json["displayState"], "engineSlow");
        assert_eq!(json["sessionId"], 1);
        assert_eq!(json["captureId"], 7);
    }

    // A zero here reads as "no engine ran", which is what the lifecycle notices
    // mean. This line did run an engine - it ran it for the whole deadline.
    //
    // `total_ms` must stay the viewer's wait rather than a copy of the deadline:
    // everywhere else in the readout it is measured from capture, so a copy here
    // would under-report by the capture and OCR time and quietly mean something
    // else than the same field on the frame beside it.
    #[test]
    fn an_abandoned_line_reports_the_time_it_was_given_and_the_wait_it_cost() {
        let payload = TranslationPayload::engine_slow(1, 7, 5_000, 5_400);

        assert_eq!(payload.model_ms, 5_000);
        assert_eq!(payload.total_ms, 5_400);
    }

    // Nothing was translated, so nothing may look translated. `original` and
    // `translated` staying empty is what stops the overlay putting OCR text on
    // screen as if a backend had produced it.
    #[test]
    fn an_abandoned_line_carries_no_text_and_names_no_backend() {
        let payload = TranslationPayload::engine_slow(1, 7, 5_000, 5_400);

        assert!(payload.original.is_empty());
        assert!(payload.translated.is_empty());
        assert!(payload.backend_used.is_empty());
    }
}
