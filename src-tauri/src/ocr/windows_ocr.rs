use super::{preprocess_image, OcrError, OcrResult, PreprocessingConfig};
use crate::core_client::{self, OcrRecognizeParams};
use tracing::{debug, info, warn};

pub struct WindowsOcr {
    language: Option<String>,
    capture_scale: f64,
}

impl WindowsOcr {
    pub fn for_capture_scale(mut self, capture_scale: f64) -> Self {
        self.capture_scale = capture_scale;
        self
    }

    pub fn new() -> Result<Self, OcrError> {
        let language = core_client::ocr_initialize_blocking(None).map_err(OcrError::InitError)?;
        Ok(Self {
            language: Some(language),
            capture_scale: 1.0,
        })
    }

    pub fn with_language(language_tag: &str) -> Result<Self, OcrError> {
        let language = super::normalize_language_tag(language_tag);
        let resolved = core_client::ocr_initialize_blocking(Some(language))
            .map_err(OcrError::LanguageNotSupported)?;
        Ok(Self {
            language: Some(resolved),
            capture_scale: 1.0,
        })
    }

    pub async fn recognize(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<OcrResult, OcrError> {
        self.recognize_with_preprocessing(image_data, width, height, PreprocessingConfig::optimal())
            .await
    }

    pub async fn recognize_without_preprocessing(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<OcrResult, OcrError> {
        validate_frame(image_data, width, height)?;
        let (image_data, width, height) =
            super::frame_budget::fit_frame(image_data, width, height, self.capture_scale);
        self.recognize_raw(image_data.into_owned(), width, height)
            .await
    }

    pub async fn recognize_with_preprocessing(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
        preprocessing: PreprocessingConfig,
    ) -> Result<OcrResult, OcrError> {
        validate_frame(image_data, width, height)?;
        let (image_data, width, height) =
            super::frame_budget::fit_frame(image_data, width, height, self.capture_scale);
        let processed = preprocess_image(&image_data, width, height, preprocessing);
        self.recognize_raw(processed, width, height).await
    }

    async fn recognize_raw(
        &self,
        image_data: Vec<u8>,
        width: u32,
        height: u32,
    ) -> Result<OcrResult, OcrError> {
        let result = core_client::ocr_recognize(
            OcrRecognizeParams {
                language: self.language.clone(),
                width,
                height,
                stride: width * 4,
                timeout_ms: 30_000,
            },
            image_data,
        )
        .await
        .map_err(OcrError::RecognitionError)?;
        let (lines, boxes) = super::line_geometry::clean_lines(result.lines, result.boxes);
        Ok(OcrResult::with_boxes(lines, boxes, result.frame_width))
    }

    pub fn available_languages() -> Result<Vec<String>, OcrError> {
        core_client::ocr_languages_blocking().map_err(OcrError::InitError)
    }
    /// Run multi-pass OCR with different preprocessing configurations.
    ///
    /// Select the pass with the most significant characters. Windows OCR does
    /// not supply confidence scores; this is a product heuristic.
    ///
    /// The different passes use varying combinations of:
    /// - Grayscale on/off
    /// - Contrast enhancement on/off
    ///
    /// # Arguments
    /// * `image_data` - Raw pixel data in BGRA format (4 bytes per pixel)
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
    /// * `pass_count` - Number of OCR passes to run (typically 2-3)
    ///
    /// # Returns
    /// The OCR result with the most significant characters
    pub async fn recognize_multi_pass(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
        pass_count: u32,
    ) -> Result<OcrResult, OcrError> {
        info!("Running multi-pass OCR with {} passes", pass_count);

        // Progressively weaker preprocessing. None of these binarize, so none
        // gets the adaptive threshold the single pass relies on.
        let configs = [
            PreprocessingConfig {
                grayscale: true,
                contrast_enhancement: true,
                binarize: false,
            },
            PreprocessingConfig {
                grayscale: true,
                contrast_enhancement: false,
                binarize: false,
            },
            PreprocessingConfig {
                grayscale: false,
                contrast_enhancement: true,
                binarize: false,
            },
            PreprocessingConfig {
                grayscale: false,
                contrast_enhancement: false,
                binarize: false,
            },
        ];

        let mut best_result: Option<OcrResult> = None;
        let mut best_score: usize = 0;

        // Run OCR with each configuration (up to pass_count configs)
        for (i, config) in configs.iter().enumerate() {
            if i >= pass_count as usize {
                break;
            }

            let pass_num = i + 1;
            debug!(
                "Multi-pass OCR: pass {}/{} with grayscale={}, contrast={}, binarize={}",
                pass_num,
                pass_count,
                config.grayscale,
                config.contrast_enhancement,
                config.binarize
            );

            match self
                .recognize_with_preprocessing(image_data, width, height, *config)
                .await
            {
                Ok(result) => {
                    // The pass that resolved the most glyphs resolved the most
                    // of the subtitle. Windows OCR reports no confidence, so
                    // there is nothing better to rank passes by.
                    let score = result.significant_chars();
                    debug!(
                        "Multi-pass OCR: pass {} resolved {} significant chars, text length = {}",
                        pass_num,
                        score,
                        result.text.len()
                    );

                    if score > best_score {
                        best_score = score;
                        best_result = Some(result);
                    }
                }
                Err(e) => {
                    warn!("Multi-pass OCR: pass {} failed with error: {}", pass_num, e);
                }
            }
        }

        // Return the best result, or an empty result if all passes failed
        match best_result {
            Some(result) => {
                info!(
                    "Multi-pass OCR complete: best pass resolved {} significant chars",
                    best_score
                );
                Ok(result)
            }
            None => {
                warn!("Multi-pass OCR: all passes failed, returning empty result");
                Ok(OcrResult::empty())
            }
        }
    }
}

// =============================================================================
// TESTS
// =============================================================================

fn validate_frame(bytes: &[u8], width: u32, height: u32) -> Result<(), OcrError> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|size| size.checked_mul(4));
    if width == 0 || height == 0 || expected != Some(bytes.len()) {
        return Err(OcrError::InvalidImage(
            "BGRA byte length does not match dimensions".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_validation_rejects_truncation_before_preprocessing() {
        assert!(validate_frame(&[0; 4], 1, 1).is_ok());
        assert!(validate_frame(&[0; 3], 1, 1).is_err());
        assert!(validate_frame(&[], 0, 1).is_err());
        assert!(validate_frame(&[], u32::MAX, u32::MAX).is_err());
    }
}
