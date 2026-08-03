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

mod language;
mod preprocessing;
mod text_cleanup;
mod windows_ocr;

pub use language::normalize_language_tag;
pub use preprocessing::*;
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
}

impl OcrResult {
    /// Create a new OCR result
    pub fn new(lines: Vec<String>) -> Self {
        let text = lines.join(" ");
        Self { text, lines }
    }

    /// Create an empty result (no text found)
    pub fn empty() -> Self {
        Self {
            text: String::new(),
            lines: Vec::new(),
        }
    }

    /// Count of letters and digits recognised.
    ///
    /// Windows OCR reports no confidence of any kind, so this is the only
    /// signal available for comparing two reads of the same frame: the pass
    /// that resolved more glyphs resolved more of the subtitle. It is a proxy
    /// for legibility and deliberately not called a confidence - the previous
    /// score of that name was a text-shape heuristic that rejected correctly
    /// recognised subtitles for being short or free of punctuation.
    pub fn significant_chars(&self) -> usize {
        self.text.chars().filter(|ch| ch.is_alphanumeric()).count()
    }

    /// Check if any text was recognized
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
}
