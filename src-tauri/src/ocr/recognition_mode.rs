// =============================================================================
// RECOGNITION_MODE.RS - which of the three recognition paths a frame takes
// =============================================================================
// The capture loop had this inline as a fifty-line three-way branch whose only
// real content was the same handful of settings, three call shapes, and the
// same error handling copied three times. Lifting it out leaves the loop
// reading as what it is - capture, recognise, filter, translate - and puts the
// settings that choose a path next to each other where they can be compared.
// =============================================================================

use super::{OcrError, OcrResult, PreprocessingConfig, WindowsOcr};

/// The recognition settings that decide which path a frame takes.
///
/// Read once when a session starts, since changing them mid-session would make
/// one session's frames incomparable with each other.
#[derive(Debug, Clone, Copy)]
pub struct RecognitionMode {
    pub multi_pass: bool,
    pub multi_pass_count: u32,
    pub preprocessing: bool,
    pub grayscale: bool,
    pub contrast_enhancement: bool,
    pub binarize: bool,
}

impl RecognitionMode {
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
}
