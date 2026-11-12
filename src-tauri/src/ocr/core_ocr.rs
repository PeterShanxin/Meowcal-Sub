use super::{preprocess_image, OcrError, OcrResult, PreprocessingConfig};
use crate::core_client::{self, OcrRecognizeParams};
use meowcal_core::ocr::Engine;
use tracing::{debug, info, warn};

/// PP-OCRv6 threads while translation runs on the GPU, and while it shares the
/// CPU. With HY-MT on the CPU and other work on the machine, four threads made
/// translation time out; two kept frames under the 250 ms capture interval.
const PP_OCR_THREADS_BESIDE_GPU: u8 = 4;
const PP_OCR_THREADS_BESIDE_CPU: u8 = 2;

/// Decided per frame, so a session whose engine finishes starting on the GPU
/// gets the larger share from then on.
fn pp_ocr_threads() -> u8 {
    if core_client::translation_on_gpu() {
        PP_OCR_THREADS_BESIDE_GPU
    } else {
        PP_OCR_THREADS_BESIDE_CPU
    }
}

/// A recognizer in Core's OCR process: Windows OCR for one language, or the
/// language-independent PP-OCRv6.
pub struct CoreOcr {
    engine: Engine,
    language: Option<String>,
    capture_scale: f64,
}

impl CoreOcr {
    pub fn for_capture_scale(mut self, capture_scale: f64) -> Self {
        self.capture_scale = capture_scale;
        self
    }

    pub fn engine(&self) -> Engine {
        self.engine
    }

    /// Windows OCR in the user-profile languages.
    pub fn new() -> Result<Self, OcrError> {
        let language = core_client::ocr_initialize_blocking(Engine::Windows, None, None)
            .map_err(OcrError::InitError)?;
        Ok(Self {
            engine: Engine::Windows,
            language,
            capture_scale: 1.0,
        })
    }

    pub fn pp_ocr() -> Result<Self, OcrError> {
        core_client::ocr_initialize_blocking(Engine::PpOcrV6, None, Some(pp_ocr_threads()))
            .map_err(OcrError::InitError)?;
        Ok(Self {
            engine: Engine::PpOcrV6,
            language: None,
            capture_scale: 1.0,
        })
    }

    /// Windows OCR in `language_tag`.
    pub fn with_language(language_tag: &str) -> Result<Self, OcrError> {
        let language = super::normalize_language_tag(language_tag);
        let resolved = core_client::ocr_initialize_blocking(Engine::Windows, Some(language), None)
            .map_err(OcrError::LanguageNotSupported)?;
        Ok(Self {
            engine: Engine::Windows,
            language: resolved,
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

    /// Recognise only the frame's white glyphs; see `glyph_mask`.
    pub async fn recognize_white_glyphs(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<OcrResult, OcrError> {
        validate_frame(image_data, width, height)?;
        let (image_data, width, height) =
            super::frame_budget::fit_frame(image_data, width, height, self.capture_scale);
        let super::glyph_mask::MaskedFrame {
            bgra,
            width: padded_width,
            height: padded_height,
            margin,
        } = super::glyph_mask::mask_white_glyphs(&image_data, width, height);
        let result = self
            .recognize_raw(bgra, padded_width, padded_height)
            .await?;
        Ok(super::glyph_mask::remove_margin(result, width, margin))
    }

    async fn recognize_raw(
        &self,
        image_data: Vec<u8>,
        width: u32,
        height: u32,
    ) -> Result<OcrResult, OcrError> {
        let evidence = crate::logging_settings::verbose().then(|| image_data.clone());
        let result = core_client::ocr_recognize(
            OcrRecognizeParams {
                engine: self.engine,
                language: self.language.clone(),
                max_threads: (self.engine == Engine::PpOcrV6).then(pp_ocr_threads),
                width,
                height,
                stride: width * 4,
                timeout_ms: 30_000,
            },
            image_data,
        )
        .await;
        if let Some(frame) = evidence {
            let detail = match &result {
                Ok(raw) => serde_json::json!({"text": raw.text, "lines": raw.lines,
                    "boxes": raw.boxes.iter().map(|b| [b.x, b.y, b.width, b.height]).collect::<Vec<_>>(),
                    "frameWidth": raw.frame_width}),
                Err(_) => serde_json::json!({"status": "recognitionFailed"}),
            };
            crate::diagnostic_artifacts::record("ocr", detail, Some((frame, width, height))).await;
        }
        let result = result.map_err(OcrError::RecognitionError)?;
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
