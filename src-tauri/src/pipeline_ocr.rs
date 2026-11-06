//! Opening the OCR client a translation session reads frames with.

use crate::event_payloads::CaptureStatusPayload;
use crate::ocr::CoreOcr;
use meowcal_core::ocr::Engine;
use tauri::{AppHandle, Emitter};
use tracing::{info, warn};

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
                            "Text recognition could not start: {e}.                              Try Windows OCR in Settings > Text recognition."
                        ),
                        is_error: true,
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
                },
            );
            CoreOcr::new()
                .inspect_err(|e| warn!("❌ Failed to initialize OCR: {}", e))
                .ok()
        }
    }
}
