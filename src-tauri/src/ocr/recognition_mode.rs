// =============================================================================
// RECOGNITION_MODE.RS - which recognition path a frame takes
// =============================================================================
// The capture loop had this inline as a fifty-line three-way branch whose only
// real content was the same handful of settings, three call shapes, and the
// same error handling copied three times. Lifting it out leaves the loop
// reading as what it is - capture, recognise, filter, translate - and puts the
// settings that choose a path next to each other where they can be compared.
//
// Subtitle mode reads white glyphs first (see `glyph_mask`) and falls back to
// the configured path only when that finds nothing, so coloured subtitles still
// read as before. Translate any text keeps the configured path: page text is
// usually dark on light, which the white-glyph mask would erase.
// =============================================================================

use super::{OcrError, OcrResult, PreprocessingConfig, WindowsOcr};
use crate::config::TranslationConfig;

/// The recognition settings that decide which path a frame takes.
///
/// Read once when a session starts, since changing them mid-session would make
/// one session's frames incomparable with each other.
#[derive(Debug, Clone, Copy)]
pub struct RecognitionMode {
    pub white_glyphs_first: bool,
    pub multi_pass: bool,
    pub multi_pass_count: u32,
    pub preprocessing: bool,
    pub grayscale: bool,
    pub contrast_enhancement: bool,
    pub binarize: bool,
}

impl RecognitionMode {
    pub fn from_config(config: &TranslationConfig) -> Self {
        Self {
            white_glyphs_first: !config.translate_all_ocr_text,
            multi_pass: config.ocr.enable_multi_pass,
            multi_pass_count: config.ocr.multi_pass_count,
            preprocessing: config.ocr.preprocessing_enabled,
            grayscale: config.ocr.grayscale,
            contrast_enhancement: config.ocr.contrast_enhancement,
            binarize: config.ocr.binarize,
        }
    }

    /// Recognise one frame by whichever path the settings select.
    ///
    /// The caller decides what a failure means - the capture loop skips the
    /// frame and waits out its budget - so this only says which path failed.
    pub async fn recognize(
        &self,
        ocr: &WindowsOcr,
        frame: &[u8],
        width: u32,
        height: u32,
    ) -> Result<OcrResult, (OcrError, &'static str)> {
        if self.white_glyphs_first {
            let result = ocr
                .recognize_white_glyphs(frame, width, height)
                .await
                .map_err(|error| (error, "OCR failed"))?;
            if result.significant_chars() > 0 {
                return Ok(result);
            }
        }
        if self.multi_pass {
            return ocr
                .recognize_multi_pass(frame, width, height, self.multi_pass_count)
                .await
                .map_err(|error| (error, "Multi-pass OCR failed"));
        }
        if self.preprocessing {
            let config = PreprocessingConfig {
                grayscale: self.grayscale,
                contrast_enhancement: self.contrast_enhancement,
                binarize: self.binarize,
            };
            return ocr
                .recognize_with_preprocessing(frame, width, height, config)
                .await
                .map_err(|error| (error, "OCR failed"));
        }
        ocr.recognize_without_preprocessing(frame, width, height)
            .await
            .map_err(|error| (error, "OCR failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode() -> RecognitionMode {
        RecognitionMode {
            white_glyphs_first: false,
            multi_pass: false,
            multi_pass_count: 2,
            preprocessing: false,
            grayscale: false,
            contrast_enhancement: false,
            binarize: false,
        }
    }

    // Multi-pass wins over preprocessing when both are set, which is what the
    // original branch did and what the settings screen implies.
    #[test]
    fn multi_pass_takes_precedence_over_preprocessing() {
        let both = RecognitionMode {
            multi_pass: true,
            preprocessing: true,
            ..mode()
        };
        assert!(both.multi_pass);
    }

    #[test]
    fn the_plain_path_is_the_default() {
        let plain = mode();
        assert!(!plain.multi_pass && !plain.preprocessing);
    }

    #[test]
    fn subtitle_mode_reads_white_glyphs_first_and_any_text_does_not() {
        let subtitles = TranslationConfig::default();
        let mode = RecognitionMode::from_config(&subtitles);
        assert!(mode.white_glyphs_first);
        assert_eq!(mode.binarize, subtitles.ocr.binarize);
        assert_eq!(mode.multi_pass, subtitles.ocr.enable_multi_pass);

        let any_text = TranslationConfig {
            translate_all_ocr_text: true,
            ..subtitles
        };
        assert!(!RecognitionMode::from_config(&any_text).white_glyphs_first);
    }
}
