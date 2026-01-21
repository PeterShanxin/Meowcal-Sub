// =============================================================================
// OCR MODULE - Optical Character Recognition
// =============================================================================
// This module reads text from images using Windows' built-in OCR engine.
//
// Windows OCR is:
// - Free (no API keys needed)
// - Fast (especially on Copilot+ PCs with NPU)
// - Supports many languages
// =============================================================================

mod windows_ocr;

pub use windows_ocr::*;

use thiserror::Error;

/// Errors that can occur during OCR
#[derive(Error, Debug)]
pub enum OcrError {
    #[error("Failed to initialize OCR engine: {0}")]
    InitError(String),

    #[error("OCR recognition failed: {0}")]
    RecognitionError(String),

    #[error("Language not supported: {0}")]
    LanguageNotSupported(String),

    #[error("Invalid image data: {0}")]
    InvalidImage(String),
}

/// The result of OCR recognition
#[derive(Debug, Clone)]
pub struct OcrResult {
    /// The recognized text (all lines joined)
    pub text: String,
    /// Individual lines of recognized text
    pub lines: Vec<String>,
    /// Confidence score (0.0 to 1.0), if available
    pub confidence: Option<f32>,
}

impl OcrResult {
    /// Create a new OCR result
    pub fn new(lines: Vec<String>) -> Self {
        let text = lines.join(" ");
        Self {
            text,
            lines,
            confidence: None,
        }
    }

    /// Create an empty result (no text found)
    pub fn empty() -> Self {
        Self {
            text: String::new(),
            lines: Vec::new(),
            confidence: None,
        }
    }

    /// Check if any text was recognized
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
}
