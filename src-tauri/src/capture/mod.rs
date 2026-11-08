// =============================================================================
// CAPTURE MODULE - Screen Capture Functionality
// =============================================================================
// This module handles capturing a portion of the screen as an image.
//
// We have two capture backends:
// 1. Windows.Graphics.Capture (PRIMARY) - Works with hardware-accelerated content
// 2. Windows GDI (FALLBACK) - Legacy method, doesn't work well with videos
// =============================================================================

mod d3d;
mod frame;
mod graphics_capture;
mod win32;

// Re-export the GDI capture for fallback
pub use win32::capture_region;

// Export the new Graphics Capture as the primary method
pub use graphics_capture::capture_region_graphics;
pub use graphics_capture::is_graphics_capture_supported;

// Export session management for persistent capture (no border flashing)
pub use graphics_capture::capture_with_session;
pub use graphics_capture::close_capture_session;
pub use graphics_capture::init_capture_session;

use thiserror::Error;

/// Errors that can occur during screen capture
#[derive(Error, Debug)]
pub enum CaptureError {
    #[error("Capture display changed")]
    DisplayChanged,
    #[error("Failed to get device context: {0}")]
    DeviceContextError(String),

    #[error("Failed to create bitmap: {0}")]
    BitmapError(String),

    #[error("Failed to capture screen: {0}")]
    CaptureError(String),

    #[error("Invalid region: {0}")]
    InvalidRegion(String),

    // New error types for Graphics Capture
    #[error("Direct3D error: {0}")]
    D3DError(String),

    #[error("Windows.Graphics.Capture error: {0}")]
    GraphicsCaptureError(String),
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
        Self {
            data,
            width,
            height,
        }
    }

    /// Get the expected size of the data buffer
    pub fn expected_size(&self) -> usize {
        (self.width * self.height * 4) as usize // 4 bytes per pixel (BGRA)
    }

    /// Check if the data buffer is the correct size
    pub fn is_valid(&self) -> bool {
        self.data.len() == self.expected_size()
    }
}

// =============================================================================
// SMART CAPTURE - Tries Graphics Capture first, falls back to GDI
// =============================================================================

use crate::capture_target::CaptureMonitor;
use crate::config::CaptureRegion;
use tracing::{info, warn};

/// Capture a region of the screen using the best available method
///
/// This function tries the Windows.Graphics.Capture API first (works with
/// hardware-accelerated video content), and falls back to GDI if that fails.
///
/// # Arguments
/// * `region` - The rectangular region to capture
///
/// # Returns
/// * `Ok((CaptureResult, bool))` - The captured image and whether fallback was used
/// * `Err(CaptureError)` - If both methods failed
pub fn smart_capture(
    monitor: &CaptureMonitor,
    region: &CaptureRegion,
) -> Result<(CaptureResult, bool), CaptureError> {
    let desktop_region = monitor
        .desktop_region(*region)
        .map_err(|e| CaptureError::InvalidRegion(e.to_string()))?;
    crate::display_monitor::handle(monitor).map_err(|_| CaptureError::DisplayChanged)?;
    // Try Graphics Capture first
    match capture_region_graphics(monitor, region) {
        Ok(result) => Ok((result, false)), // false = primary method worked
        Err(CaptureError::DisplayChanged) => Err(CaptureError::DisplayChanged),
        Err(e) => {
            warn!("Graphics Capture failed, falling back to GDI: {}", e);

            // Fall back to GDI
            crate::display_monitor::handle(monitor).map_err(|_| CaptureError::DisplayChanged)?;
            match capture_region(&desktop_region) {
                Ok(result) => {
                    info!("GDI fallback capture succeeded");
                    Ok((result, true)) // true = fallback was used
                }
                Err(gdi_error) => {
                    // Both methods failed, return the original error with context
                    Err(CaptureError::CaptureError(format!(
                        "All capture methods failed. Graphics Capture: {}. GDI: {}",
                        e, gdi_error
                    )))
                }
            }
        }
    }
}
