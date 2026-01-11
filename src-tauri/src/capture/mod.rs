// =============================================================================
// CAPTURE MODULE - Screen Capture Functionality
// =============================================================================
// This module handles capturing a portion of the screen as an image.
// We use Windows GDI (Graphics Device Interface) APIs for this.
// =============================================================================

mod win32;

pub use win32::*;

use thiserror::Error;

/// Errors that can occur during screen capture
#[derive(Error, Debug)]
pub enum CaptureError {
    #[error("Failed to get device context: {0}")]
    DeviceContextError(String),
    
    #[error("Failed to create bitmap: {0}")]
    BitmapError(String),
    
    #[error("Failed to capture screen: {0}")]
    CaptureError(String),
    
    #[error("Invalid region: {0}")]
    InvalidRegion(String),
}

/// The result of a screen capture
#[derive(Debug)]
pub struct CaptureResult {
    /// Raw pixel data in BGRA format (4 bytes per pixel)
    pub data: Vec<u8>,
    /// Width of the captured image
    pub width: u32,
    /// Height of the captured image
    pub height: u32,
}

impl CaptureResult {
    /// Create a new capture result
    pub fn new(data: Vec<u8>, width: u32, height: u32) -> Self {
        Self { data, width, height }
    }
    
    /// Get the expected size of the data buffer
    pub fn expected_size(&self) -> usize {
        (self.width * self.height * 4) as usize  // 4 bytes per pixel (BGRA)
    }
    
    /// Check if the data buffer is the correct size
    pub fn is_valid(&self) -> bool {
        self.data.len() == self.expected_size()
    }
}
