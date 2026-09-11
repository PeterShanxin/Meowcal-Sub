//! Native recognition of tightly packed BGRA8 frames. Capture, preprocessing,
//! text cleanup and pass selection belong to the consuming product.

mod geometry;
mod native;

pub use native::NativeOcr;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_DIMENSION: u32 = 4096;
pub const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum OcrError {
    #[error("Failed to initialize OCR: {0}")]
    Init(String),
    #[error("OCR language unavailable: {0}")]
    Language(String),
    #[error("Invalid BGRA frame: {0}")]
    Frame(String),
    #[error("OCR recognition failed: {0}")]
    Recognition(String),
    #[error("OCR recognition timed out; this process must be restarted")]
    Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LineBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeResult {
    pub text: String,
    pub lines: Vec<String>,
    pub boxes: Vec<LineBox>,
    pub frame_width: f32,
}

/// BGRA8 is top-to-bottom, with no row padding. Alpha is ignored by OCR.
pub fn frame_bytes(width: u32, height: u32, stride: u32) -> Result<usize, OcrError> {
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(OcrError::Frame("dimensions must be within 1..=4096".into()));
    }
    if stride != width * 4 {
        return Err(OcrError::Frame("stride must equal width * 4".into()));
    }
    Ok(stride as usize * height as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_contract_rejects_empty_overflow_and_padded_rows() {
        for (w, h, stride) in [
            (0, 1, 0),
            (1, 0, 4),
            (4097, 1, 16388),
            (u32::MAX, 1, 0),
            (2, 1, 12),
        ] {
            assert!(frame_bytes(w, h, stride).is_err());
        }
        assert_eq!(frame_bytes(4096, 4096, 16384).unwrap(), MAX_FRAME_BYTES);
        assert_eq!(frame_bytes(2, 3, 8).unwrap(), 24);
    }
}
