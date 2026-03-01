// =============================================================================
// WINDOWS_OCR.RS - Windows.Media.Ocr Implementation
// =============================================================================
// This file uses the Windows Runtime (WinRT) OCR APIs.
//
// These are the same APIs that Live Captions and other Windows features use!
// On Copilot+ PCs, some of these operations run on the NPU for better battery.
// =============================================================================

use super::{OcrError, OcrResult, PreprocessingConfig, preprocess_image};
use tracing::{debug, info, warn};

// Windows Runtime APIs for OCR
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Security::Cryptography::CryptographicBuffer;

/// WindowsOcr - The main OCR engine wrapper
///
/// This wraps the Windows.Media.Ocr.OcrEngine to make it easier to use.
pub struct WindowsOcr {
    engine: OcrEngine,
}

impl WindowsOcr {
    /// Create a new OCR engine using the system's default language
    ///
    /// This will use whatever languages you have installed in Windows Settings.
    ///
    /// # Example
    /// ```rust,no_run
    /// use meowcal_sub::ocr::WindowsOcr;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let ocr = WindowsOcr::new()?;
    ///     Ok(())
    /// }
    /// ```
    pub fn new() -> Result<Self, OcrError> {
        info!("Initializing Windows OCR with user profile languages...");

        // Try to create an engine using the user's preferred languages
        let engine = OcrEngine::TryCreateFromUserProfileLanguages()
            .map_err(|e| OcrError::InitError(format!("Failed to create OCR engine: {}", e)))?;

        if let Ok(lang) = engine.RecognizerLanguage() {
            if let Ok(tag) = lang.LanguageTag() {
                info!("OCR engine initialized with language: {:?}", tag);
            }
        }

        Ok(Self { engine })
    }

    /// Create an OCR engine with a specific language
    ///
    /// # Arguments
    /// * `language_tag` - BCP-47 language tag like "en-US", "ja-JP", "zh-CN"
    ///
    /// # Example
    /// ```rust,no_run
    /// use meowcal_sub::ocr::WindowsOcr;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let ocr = WindowsOcr::with_language("ja-JP")?;
    ///     Ok(())
    /// }
    /// ```
    pub fn with_language(language_tag: &str) -> Result<Self, OcrError> {
        info!("Initializing Windows OCR with language: {}", language_tag);

        // Create a Windows Language object from the tag
        let language = Language::CreateLanguage(&language_tag.into()).map_err(|e| {
            OcrError::LanguageNotSupported(format!(
                "Invalid language tag '{}': {}",
                language_tag, e
            ))
        })?;

        // Check if this language is available for OCR
        if !OcrEngine::IsLanguageSupported(&language)
            .map_err(|e| OcrError::InitError(e.to_string()))?
        {
            return Err(OcrError::LanguageNotSupported(format!(
                "Language '{}' is not installed for OCR. Install it in Windows Settings.",
                language_tag
            )));
        }

        // Create the engine
        let engine = OcrEngine::TryCreateFromLanguage(&language)
            .map_err(|e| OcrError::InitError(format!("Failed to create OCR engine: {}", e)))?;

        info!("OCR engine created successfully for '{}'", language_tag);
        Ok(Self { engine })
    }

    /// Recognize text in an image with preprocessing enabled by default.
    ///
    /// This method applies image preprocessing (grayscale + contrast enhancement)
    /// before OCR to improve accuracy. Use `recognize_without_preprocessing` if you
    /// want to skip preprocessing.
    ///
    /// # Arguments
    /// * `image_data` - Raw pixel data in BGRA format (4 bytes per pixel)
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
    ///
    /// # Returns
    /// The recognized text
    ///
    /// # Example
    /// ```rust,no_run
    /// use meowcal_sub::capture::capture_region;
    /// use meowcal_sub::config::CaptureRegion;
    /// use meowcal_sub::ocr::WindowsOcr;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let ocr = WindowsOcr::new()?;
    ///     let region = CaptureRegion::new(0, 0, 800, 100);
    ///     let capture = capture_region(&region)?;
    ///
    ///     let runtime = tokio::runtime::Runtime::new()?;
    ///     let result = runtime.block_on(async {
    ///         ocr.recognize(&capture.data, capture.width, capture.height).await
    ///     })?;
    ///
    ///     println!("Found text: {}", result.text);
    ///     Ok(())
    /// }
    /// ```
    pub async fn recognize(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<OcrResult, OcrError> {
        // Use preprocessing by default (optimal settings for most images)
        self.recognize_with_preprocessing(image_data, width, height, PreprocessingConfig::optimal()).await
    }

    /// Recognize text in an image without any preprocessing.
    ///
    /// Use this when you want to process the image yourself or when preprocessing
    /// is causing issues with your specific use case.
    pub async fn recognize_without_preprocessing(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<OcrResult, OcrError> {
        self.recognize_raw(image_data, width, height).await
    }

    /// Recognize text in an image with preprocessing configuration.
    ///
    /// This method applies image preprocessing (grayscale, contrast enhancement)
    /// before OCR to improve accuracy on noisy or low-contrast images.
    ///
    /// # Arguments
    /// * `image_data` - Raw pixel data in BGRA format (4 bytes per pixel)
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
    /// * `preprocessing` - Preprocessing configuration options
    ///
    /// # Returns
    /// The recognized text with confidence score
    pub async fn recognize_with_preprocessing(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
        preprocessing: PreprocessingConfig,
    ) -> Result<OcrResult, OcrError> {
        // Apply preprocessing if enabled
        let processed_data = preprocess_image(image_data, width, height, preprocessing);

        // Use the processed data for OCR
        self.recognize_raw(&processed_data, width, height).await
    }

    /// Internal method to recognize text from already-processed image data.
    async fn recognize_raw(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<OcrResult, OcrError> {
        debug!(
            "Recognizing text in {}x{} image ({} bytes)",
            width,
            height,
            image_data.len()
        );

        // Validate input
        let expected_size = (width * height * 4) as usize;
        if image_data.len() != expected_size {
            return Err(OcrError::InvalidImage(format!(
                "Expected {} bytes for {}x{} BGRA image, got {}",
                expected_size,
                width,
                height,
                image_data.len()
            )));
        }

        // Convert our raw bytes to a Windows IBuffer
        // CryptographicBuffer is a convenient way to do this
        let buffer = CryptographicBuffer::CreateFromByteArray(image_data)
            .map_err(|e| OcrError::InvalidImage(format!("Failed to create buffer: {}", e)))?;

        // Create a SoftwareBitmap from the buffer
        // This is the format that OcrEngine.RecognizeAsync expects
        let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
            &buffer,
            BitmapPixelFormat::Bgra8, // Our capture is in BGRA format
            width as i32,
            height as i32,
        )
        .map_err(|e| OcrError::InvalidImage(format!("Failed to create bitmap: {}", e)))?;

        // Run OCR!
        let ocr_result = self
            .engine
            .RecognizeAsync(&bitmap)
            .map_err(|e| OcrError::RecognitionError(format!("RecognizeAsync failed: {}", e)))?
            .get() // Block until complete (IAsyncOperation -> Result)
            .map_err(|e| OcrError::RecognitionError(format!("OCR failed: {}", e)))?;

        // Extract the text from each line
        let lines_collection = ocr_result
            .Lines()
            .map_err(|e| OcrError::RecognitionError(format!("Failed to get lines: {}", e)))?;

        let mut lines = Vec::new();
        for i in 0..lines_collection.Size().unwrap_or(0) {
            if let Ok(line) = lines_collection.GetAt(i) {
                if let Ok(text) = line.Text() {
                    let text_str = text.to_string();
                    if !text_str.trim().is_empty() {
                        lines.push(text_str);
                    }
                }
            }
        }

        // Create the result
        let mut result = OcrResult::new(lines);

        // Calculate confidence score using heuristic method
        // Windows OCR doesn't provide native confidence, so we estimate it
        result.confidence = Some(result.calculate_confidence());

        debug!(
            "OCR found {} lines ({} chars), confidence: {:.2}",
            result.lines.len(),
            result.text.chars().count(),
            result.confidence.unwrap_or(0.0)
        );

        Ok(result)
    }

    /// Get a list of available OCR languages on this system
    pub fn available_languages() -> Result<Vec<String>, OcrError> {
        let languages = OcrEngine::AvailableRecognizerLanguages()
            .map_err(|e| OcrError::InitError(format!("Failed to get languages: {}", e)))?;

        let mut result = Vec::new();
        for i in 0..languages.Size().unwrap_or(0) {
            if let Ok(lang) = languages.GetAt(i) {
                if let Ok(tag) = lang.LanguageTag() {
                    result.push(tag.to_string());
                }
            }
        }

        Ok(result)
    }

    /// Run multi-pass OCR with different preprocessing configurations.
    ///
    /// This method runs OCR multiple times with different preprocessing settings
    /// and selects the result with the highest confidence score.
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
    /// The best OCR result based on confidence score
    pub async fn recognize_multi_pass(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
        pass_count: u32,
    ) -> Result<OcrResult, OcrError> {
        info!("Running multi-pass OCR with {} passes", pass_count);

        // Define different preprocessing configurations to try
        // Pass 1: Standard preprocessing (grayscale + contrast)
        // Pass 2: Grayscale only (no contrast enhancement)
        // Pass 3: Contrast only (no grayscale)
        // Pass 4: No preprocessing (raw image)
        let configs = vec![
            PreprocessingConfig {
                grayscale: true,
                contrast_enhancement: true,
            },
            PreprocessingConfig {
                grayscale: true,
                contrast_enhancement: false,
            },
            PreprocessingConfig {
                grayscale: false,
                contrast_enhancement: true,
            },
            PreprocessingConfig {
                grayscale: false,
                contrast_enhancement: false,
            },
        ];

        let mut best_result: Option<OcrResult> = None;
        let mut best_confidence: f32 = 0.0;

        // Run OCR with each configuration (up to pass_count configs)
        for (i, config) in configs.iter().enumerate() {
            if i >= pass_count as usize {
                break;
            }

            let pass_num = i + 1;
            debug!(
                "Multi-pass OCR: pass {}/{} with grayscale={}, contrast={}",
                pass_num,
                pass_count,
                config.grayscale,
                config.contrast_enhancement
            );

            match self
                .recognize_with_preprocessing(image_data, width, height, *config)
                .await
            {
                Ok(result) => {
                    let confidence = result.confidence.unwrap_or(0.0);
                    debug!(
                        "Multi-pass OCR: pass {} result confidence = {:.2}, text length = {}",
                        pass_num,
                        confidence,
                        result.text.len()
                    );

                    if confidence > best_confidence {
                        best_confidence = confidence;
                        best_result = Some(result);
                    }
                }
                Err(e) => {
                    warn!(
                        "Multi-pass OCR: pass {} failed with error: {}",
                        pass_num, e
                    );
                }
            }
        }

        // Return the best result, or an empty result if all passes failed
        match best_result {
            Some(result) => {
                info!(
                    "Multi-pass OCR complete: best confidence = {:.2}",
                    best_confidence
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_available_languages() {
        let languages = WindowsOcr::available_languages();
        assert!(languages.is_ok(), "Should be able to list languages");

        let langs = languages.unwrap();
        println!("Available OCR languages: {:?}", langs);

        // There should be at least one language installed
        assert!(
            !langs.is_empty(),
            "At least one OCR language should be installed"
        );
    }

    #[test]
    fn test_create_ocr_engine() {
        let ocr = WindowsOcr::new();
        assert!(
            ocr.is_ok(),
            "Should be able to create OCR engine: {:?}",
            ocr.err()
        );
    }

    #[tokio::test]
    async fn test_ocr_empty_image() {
        let ocr = WindowsOcr::new().expect("Failed to create OCR");

        // Create a blank white image (100x100)
        let width = 100u32;
        let height = 100u32;
        let image_data = vec![255u8; (width * height * 4) as usize]; // All white, BGRA

        let result = ocr.recognize(&image_data, width, height).await;
        assert!(
            result.is_ok(),
            "OCR should succeed on blank image: {:?}",
            result.err()
        );

        // A blank white image should have no text
        let ocr_result = result.unwrap();
        assert!(
            ocr_result.is_empty() || ocr_result.text.trim().is_empty(),
            "Blank image should have no text"
        );
    }
}
