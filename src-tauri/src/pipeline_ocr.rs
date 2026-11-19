//! Opening the OCR client a translation session reads frames with.

use crate::event_payloads::CaptureStatusPayload;
use crate::ocr::CoreOcr;
use meowcal_core::ocr::Engine;
use tauri::{AppHandle, Emitter};
use tracing::{info, warn};

pub(crate) fn log_rejection(
    token: crate::pipeline_session::PipelineToken,
    reason: crate::ocr_gate::OcrRejection,
    text: &str,
    min_chars: usize,
) {
    tracing::debug!(
        session_id = token.session_id,
        capture_id = token.capture_id,
        reason = reason.as_str(),
        source_chars = text.chars().count(),
        min_chars,
        "OCR text rejected"
    );
}

/// The session's OCR client, or `None` when no recognizer could start; the UI
/// is told why either way.
///
/// When Windows OCR has no recognizer for `source_language`, this falls back
/// to the user-profile languages.
pub(crate) fn open(app: &AppHandle, engine: Engine, source_language: &str) -> Option<CoreOcr> {
    if engine == Engine::PpOcrV6 {
        return match CoreOcr::pp_ocr() {
            Ok(ocr) => {
                info!("OCR initialized with PP-OCRv6");
                Some(ocr)
            }
            Err(e) => {
                warn!("❌ Failed to initialize PP-OCRv6: {}", e);
                let _ = app.emit(
                    "capture-status",
                    CaptureStatusPayload {
                        using_fallback: false,
                        message: format!(
                            "Text recognition could not start: {e}. \
                             Try the Compatibility profile in Settings."
                        ),
                        is_error: true,
                        fault_code: Some("OCR_INIT_FAILED"),
                        feedback_token: None,
                    },
                );
                None
            }
        };
    }
    match CoreOcr::with_language(source_language) {
        Ok(ocr) => {
            info!("OCR initialized with language: {}", source_language);
            Some(ocr)
        }
        Err(e) => {
            warn!(
                "⚠️ OCR language '{}' not available: {}. Falling back to user profile languages.",
                source_language, e
            );
            let _ = app.emit(
                "capture-status",
                CaptureStatusPayload {
                    using_fallback: true,
                    message: format!(
                        "OCR language '{}' is not installed. Using system default. \
                         Install it: Windows Settings > Time & Language > Language & Region.",
                        source_language
                    ),
                    is_error: false,
                    fault_code: None,
                    feedback_token: None,
                },
            );
            CoreOcr::new()
                .inspect_err(|e| warn!("❌ Failed to initialize OCR: {}", e))
                .ok()
        }
    }
}
