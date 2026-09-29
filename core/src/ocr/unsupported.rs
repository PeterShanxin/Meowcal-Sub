use super::{NativeResult, OcrError};
use std::time::Duration;

/// Native OCR is unavailable on non-Windows hosts; no instance can be created.
pub struct NativeOcr {
    _private: (),
}

impl NativeOcr {
    pub fn new(_language_tag: Option<&str>) -> Result<Self, OcrError> {
        Err(OcrError::UnsupportedPlatform)
    }

    pub fn available_languages() -> Result<Vec<String>, OcrError> {
        Err(OcrError::UnsupportedPlatform)
    }

    pub fn language(&self) -> Result<String, OcrError> {
        Err(OcrError::UnsupportedPlatform)
    }

    pub fn is_poisoned(&self) -> bool {
        true
    }

    pub fn recognize_bgra(
        &mut self,
        _bytes: &[u8],
        _width: u32,
        _height: u32,
        _timeout: Duration,
    ) -> Result<NativeResult, OcrError> {
        Err(OcrError::UnsupportedPlatform)
    }
}
