//! Opening the OCR client a translation session reads frames with.

use crate::event_payloads::CaptureStatusPayload;
use crate::ocr::CoreOcr;
use meowcal_core::ocr::Engine;
use tauri::{AppHandle, Emitter};
use tracing::{info, warn};

/// The session's OCR client, or `None` when no recognizer could start.
///
/// When Windows OCR has no recognizer for `source_language`, this falls back
/// to the user-profile languages and tells the UI why.
pub(crate) fn open(app: &AppHandle, engine: Engine, source_language: &str) -> Option<CoreOcr> {
    if engine == Engine::PpOcrV6 {
        return CoreOcr::pp_ocr()
            .inspect(|_| info!("OCR initialized with PP-OCRv6"))
            .inspect_err(|e| warn!("❌ Failed to initialize PP-OCRv6: {}", e))
            .ok();
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
