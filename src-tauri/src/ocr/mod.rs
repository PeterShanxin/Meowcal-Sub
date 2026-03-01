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

mod preprocessing;
mod windows_ocr;

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

    /// Calculate heuristic confidence score based on text quality.
    ///
    /// Since Windows OCR doesn't provide native confidence scores, we use
    /// text quality heuristics to estimate reliability:
    /// - Character variety (ratio of alphanumeric to total characters)
    /// - Text length (longer, valid-looking text is more likely correct)
    /// - Presence of common punctuation and spacing patterns
    ///
    /// Returns a value between 0.0 and 1.0.
    pub fn calculate_confidence(&self) -> f32 {
        if self.is_empty() {
            return 0.0;
        }

        let text = &self.text;
        let total_chars = text.chars().count();

        if total_chars == 0 {
            return 0.0;
        }

        // Count alphanumeric characters (letters and numbers)
        let alphanumeric_count = text
            .chars()
            .filter(|ch| ch.is_alphanumeric())
            .count();

        // Base score from character variety
        let char_variety = alphanumeric_count as f32 / total_chars as f32;

        // Length bonus: longer valid-looking text is more reliable
        // Cap at 50 characters for full bonus
        let length_factor = (total_chars.min(50) as f32) / 50.0;

        // Check for common OCR noise patterns
        let has_repeated_chars = text.contains("|||") || text.contains("---") || text.contains("...");
        let has_unusual_spacing = text.contains("  ") && !text.contains('\n');

        // Punctuation factor: proper use of punctuation suggests valid text
        let punctuation_count = text.chars().filter(|ch| {
            matches!(ch, '.' | ',' | '!' | '?' | ':' | ';' | '"' | '\'' | '(' | ')' | '[' | ']')
        }).count();
        let punctuation_factor = if total_chars > 0 {
            (punctuation_count as f32 / total_chars as f32).min(0.3)
        } else {
            0.0
        };

        // Calculate final confidence
        let mut confidence = (char_variety * 0.5) + (length_factor * 0.3) + (punctuation_factor);

        // Reduce confidence for noise patterns
        if has_repeated_chars {
            confidence *= 0.7;
        }
        if has_unusual_spacing {
            confidence *= 0.8;
        }

        // Ensure result is in valid range
        confidence.clamp(0.0, 1.0)
    }
}
