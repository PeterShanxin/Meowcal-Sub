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

mod band_log;
pub mod frame_budget;
mod language;
mod line_geometry;
mod preprocessing;
mod text_cleanup;
mod windows_ocr;

/// Operator-run comparison of preprocessing variants against a real captured
/// frame. See the module header; it asserts nothing and is `#[ignore]`d.
#[cfg(test)]
mod preprocessing_ab;

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

/// Where a recognised line sat in the frame, in captured pixels.
///
/// Windows OCR reports a rectangle per word; this is their union across one
/// line. The pipeline has always thrown this away and kept only the string,
/// which is why a capture region taller than one subtitle can only be handled
/// by refusing it. Position is what tells two stacked subtitle positions apart
/// from each other and from the page furniture between them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl LineBox {
    /// Vertical centre, which is what bands are grouped on.
    pub fn middle_y(&self) -> f32 {
        self.y + self.height / 2.0
    }
}

/// The result of OCR recognition
#[derive(Debug, Clone)]
pub struct OcrResult {
    /// The recognized text (all lines joined)
    pub text: String,
    /// Individual lines of recognized text
    pub lines: Vec<String>,
    /// Where each line sat, parallel to `lines`.
    ///
    /// Empty when the source could not report geometry, so a consumer must
    /// treat it as optional rather than index it against `lines` blindly.
    pub boxes: Vec<LineBox>,
}

impl OcrResult {
    /// Create a new OCR result
    ///
    /// The edge trim runs again over the joined text. `clean_line` already ran
    /// per line, but when Windows returns a stray character as its own line it
    /// arrives as a single-token line with no CJK neighbour to be judged
    /// against, and is correctly left alone - a line that really is just "0"
    /// should survive. Joining then reinstates it beside the dialogue, which is
    /// how `虽然想完全复刻再展开的 0` reached the translator and put a bare 0
    /// inside translated dialogue.
    pub fn new(lines: Vec<String>) -> Self {
        Self::with_boxes(lines, Vec::new())
    }

    /// Create a result that also knows where each line sat.
    pub fn with_boxes(lines: Vec<String>, boxes: Vec<LineBox>) -> Self {
        let text = text_cleanup::trim_edge_noise(&lines.join(" "));
        Self { text, lines, boxes }
    }

    /// Create an empty result (no text found)
    pub fn empty() -> Self {
        Self {
            text: String::new(),
            lines: Vec::new(),
            boxes: Vec::new(),
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
