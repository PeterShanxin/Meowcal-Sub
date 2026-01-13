// =============================================================================
// WIN32.RS - Windows Screen Capture using GDI
// =============================================================================
// This file contains the actual screen capture implementation using Win32 APIs.
// 
// How screen capture works on Windows:
// 1. Get a "Device Context" (DC) for the screen - this is like a handle to draw
// 2. Create a compatible bitmap to store the capture
// 3. Copy (BitBlt) from the screen DC to our bitmap
// 4. Read the pixel data from the bitmap
// =============================================================================

use super::{CaptureError, CaptureResult};
use crate::config::CaptureRegion;
use tracing::debug;

// Import Windows APIs
// These are generated from Windows metadata and give us access to GDI functions
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
    GetDC, GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, SRCCOPY, HGDIOBJ,
};

/// Capture a region of the screen
/// 
/// # Arguments
/// * `region` - The rectangular region to capture
/// 
/// # Returns
/// * `Ok(CaptureResult)` - The captured image data
/// * `Err(CaptureError)` - If capture failed
/// 
/// # Example
/// ```rust,no_run
/// use meowcal_sub::capture::capture_region;
/// use meowcal_sub::config::CaptureRegion;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let region = CaptureRegion::new(0, 0, 800, 100);
///     let result = capture_region(&region)?;
///     println!("Captured {}x{} image", result.width, result.height);
///     Ok(())
/// }
/// ```
pub fn capture_region(region: &CaptureRegion) -> Result<CaptureResult, CaptureError> {
    // Validate the region
    if !region.is_valid() {
        return Err(CaptureError::InvalidRegion(
            format!("Invalid dimensions: {}x{}", region.width, region.height)
        ));
    }
    
    debug!("Capturing region: {:?}", region);
    
    // Use unsafe block for Win32 APIs (they're C APIs, so Rust can't guarantee safety)
    unsafe {
        // Step 1: Get the screen device context
        // None means the entire desktop
        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            return Err(CaptureError::DeviceContextError(
                "Failed to get screen DC".to_string()
            ));
        }
        
        // Step 2: Create a memory DC compatible with the screen
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        if mem_dc.is_invalid() {
            let _ = ReleaseDC(None, screen_dc);
            return Err(CaptureError::DeviceContextError(
                "Failed to create memory DC".to_string()
            ));
        }
        
        // Step 3: Create a bitmap to store the capture
        let bitmap = CreateCompatibleBitmap(screen_dc, region.width, region.height);
        if bitmap.is_invalid() {
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);
            return Err(CaptureError::BitmapError(
                "Failed to create bitmap".to_string()
            ));
        }
        
        // Step 4: Select the bitmap into the memory DC
        let old_bitmap = SelectObject(mem_dc, HGDIOBJ(bitmap.0));
        
        // Step 5: Copy from screen to our bitmap (this is the actual capture!)
        // SRCCOPY means just copy the pixels directly
        let success = BitBlt(
            mem_dc,             // Destination DC
            0, 0,               // Destination position
            region.width,       // Width to copy
            region.height,      // Height to copy
            Some(screen_dc),    // Source DC (the screen)
            region.x,           // Source X
            region.y,           // Source Y
            SRCCOPY,            // Copy operation
        );
        
        if success.is_err() {
            SelectObject(mem_dc, old_bitmap);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);
            return Err(CaptureError::CaptureError(
                "BitBlt failed".to_string()
            ));
        }
        
        // Step 6: Read the pixel data from the bitmap
        let mut bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: region.width,
                biHeight: -region.height,  // Negative = top-down DIB (normal orientation)
                biPlanes: 1,
                biBitCount: 32,            // 32 bits per pixel (BGRA)
                biCompression: BI_RGB.0,   // Uncompressed RGB
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [Default::default()],
        };
        
        // Allocate buffer for pixel data
        let buffer_size = (region.width * region.height * 4) as usize;
        let mut buffer: Vec<u8> = vec![0; buffer_size];
        
        // Get the bits!
        let lines = GetDIBits(
            mem_dc,
            bitmap,
            0,
            region.height as u32,
            Some(buffer.as_mut_ptr() as *mut _),
            &mut bitmap_info,
            DIB_RGB_COLORS,
        );
        
        // Step 7: Clean up (very important to avoid memory leaks!)
        SelectObject(mem_dc, old_bitmap);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, screen_dc);
        
        if lines == 0 {
            return Err(CaptureError::CaptureError(
                "GetDIBits failed".to_string()
            ));
        }
        
        debug!("Captured {} lines, {} bytes", lines, buffer.len());
        
        Ok(CaptureResult::new(
            buffer,
            region.width as u32,
            region.height as u32,
        ))
    }
}

/// Get the dimensions of the primary screen
/// 
/// # Returns
/// A tuple of (width, height) in pixels
pub fn get_screen_dimensions() -> (i32, i32) {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    
    unsafe {
        let width = GetSystemMetrics(SM_CXSCREEN);
        let height = GetSystemMetrics(SM_CYSCREEN);
        (width, height)
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_get_screen_dimensions() {
        let (width, height) = get_screen_dimensions();
        // Screen should have positive dimensions
        assert!(width > 0, "Screen width should be positive");
        assert!(height > 0, "Screen height should be positive");
        println!("Screen size: {}x{}", width, height);
    }
    
    #[test]
    fn test_capture_small_region() {
        // Capture a small 100x100 region from the top-left corner
        let region = CaptureRegion::new(0, 0, 100, 100);
        let result = capture_region(&region);
        
        assert!(result.is_ok(), "Capture should succeed");
        
        let capture = result.unwrap();
        assert_eq!(capture.width, 100);
        assert_eq!(capture.height, 100);
        assert!(capture.is_valid(), "Capture data should be valid size");
    }
    
    #[test]
    fn test_capture_invalid_region() {
        // Try to capture with invalid dimensions
        let region = CaptureRegion::new(0, 0, 0, 100);
        let result = capture_region(&region);
        
        assert!(result.is_err(), "Capture should fail for invalid region");
    }
}
