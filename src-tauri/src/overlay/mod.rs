// =============================================================================
// OVERLAY MODULE - Floating Subtitle Window
// =============================================================================
// This module manages the floating overlay window that displays:
// 1. A border around the capture region
// 2. Translated subtitles below the capture region
//
// The overlay is a fullscreen transparent window that is click-through.
// =============================================================================

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use tracing::{debug, info};

use crate::config::CaptureRegion;

// =============================================================================
// OVERLAY PAYLOADS (sent to frontend)
// =============================================================================

/// Payload for updating the overlay region
#[derive(Clone, Serialize)]
pub struct OverlayRegionPayload {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl From<&CaptureRegion> for OverlayRegionPayload {
    fn from(region: &CaptureRegion) -> Self {
        Self {
            x: region.x,
            y: region.y,
            width: region.width,
            height: region.height,
        }
    }
}

// =============================================================================
// OVERLAY FUNCTIONS
// =============================================================================

/// Get the overlay window handle
fn get_overlay_window(app: &AppHandle) -> Option<WebviewWindow> {
    let window = app.get_webview_window("overlay");
    if window.is_none() {
        info!("⚠️ Overlay window 'overlay' not found in app windows");
    }
    window
}

/// Configure the overlay window as a chromeless popup covering the full screen.
///
/// This approach:
/// 1. Sets WS_POPUP style - removes all window chrome (titlebar, borders)
/// 2. Covers the entire virtual screen (all monitors)
/// 3. Sets NonRudeHWND property - prevents Windows from hiding the taskbar
///
/// This is how professional overlays (OBS, Discord, game overlays) work.
#[cfg(windows)]
fn configure_overlay_as_chromeless_popup(window: &WebviewWindow) -> Result<(), String> {
    use raw_window_handle::HasWindowHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetPropW, SetWindowLongPtrW, SetWindowPos,
        GWL_STYLE, WS_POPUP, WS_VISIBLE,
        SWP_FRAMECHANGED, SWP_NOZORDER,
        GetSystemMetrics, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
        SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    };
    use windows::core::w;

    // Get the raw window handle from Tauri
    let handle = window
        .window_handle()
        .map_err(|e| format!("Failed to get window handle: {}", e))?;

    let raw_handle = handle.as_raw();

    // Extract HWND from the raw handle (Windows-specific)
    if let raw_window_handle::RawWindowHandle::Win32(win32_handle) = raw_handle {
        let hwnd_ptr = win32_handle.hwnd.get() as isize;
        let hwnd = windows::Win32::Foundation::HWND(hwnd_ptr as *mut _);

        // SAFETY: We have a valid HWND from Tauri's window handle
        unsafe {
            // 1. Set WS_POPUP style - this removes ALL window chrome
            // WS_POPUP creates a borderless, titlebar-less window
            let new_style = WS_POPUP.0 | WS_VISIBLE.0;
            SetWindowLongPtrW(hwnd, GWL_STYLE, new_style as isize);

            // 2. Get virtual screen bounds (covers all monitors)
            let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);

            info!("Virtual screen: ({}, {}) {}x{}", x, y, width, height);

            // 3. Resize and reposition to cover full virtual screen
            // SWP_FRAMECHANGED forces Windows to recalculate the frame after style change
            // Using None for hwndInsertAfter with SWP_NOZORDER keeps current z-order
            SetWindowPos(
                hwnd,
                None,  // hwndInsertAfter - ignored due to SWP_NOZORDER
                x, y, width, height,
                SWP_FRAMECHANGED | SWP_NOZORDER,
            ).map_err(|e| format!("SetWindowPos failed: {}", e))?;

            // 4. Set NonRudeHWND property - tells Windows Shell to not treat
            // this window as a fullscreen "rude" window that should hide the taskbar
            SetPropW(hwnd, w!("NonRudeHWND"), Some(HANDLE(1 as *mut _)))
                .map_err(|e| format!("SetPropW NonRudeHWND failed: {}", e))?;
        }

        info!("Configured overlay as chromeless popup with NonRudeHWND");
        Ok(())
    } else {
        Err("Window handle is not Win32".to_string())
    }
}

/// Show the overlay window
///
/// Note: Click-through is NOT enabled by default so the settings button can be clicked.
/// The frontend will manage click-through state based on user interaction.
pub fn show_overlay(app: &AppHandle) -> Result<(), String> {
    info!("🔍 Looking for overlay window...");

    let window = match get_overlay_window(app) {
        Some(w) => {
            info!("✅ Found overlay window");
            w
        }
        None => {
            let err = "Overlay window not found - check tauri.conf.json";
            info!("❌ {}", err);
            return Err(err.to_string());
        }
    };

    // Don't set click-through here - let the frontend manage it
    // This allows the settings button to be clickable
    if let Err(e) = window.set_decorations(false) {
        info!("⚠️ Failed to enforce overlay decorations: {}", e);
    }
    // NOTE: Removed set_title("") and set_focusable(false) - these can cause
    // window Chrome to briefly appear on some Windows versions. The WS_POPUP
    // style set by configure_overlay_as_chromeless_popup handles this properly.

    // Configure as chromeless popup BEFORE showing
    // This sets WS_POPUP style, covers full screen, and applies NonRudeHWND
    #[cfg(windows)]
    if let Err(e) = configure_overlay_as_chromeless_popup(&window) {
        tracing::warn!("Failed to configure overlay as chromeless popup: {}", e);
        // Continue anyway - overlay will work but may have visual issues
    }

    // Show the window
    window.show()
        .map_err(|e| format!("Failed to show overlay: {}", e))?;

    // Re-apply WS_POPUP style AFTER showing to ensure it persists
    // This prevents any window Chrome from appearing when the window gains focus
    #[cfg(windows)]
    if let Err(e) = configure_overlay_as_chromeless_popup(&window) {
        tracing::warn!("Failed to re-apply overlay popup style after show: {}", e);
    }

    // Emit visibility event
    app.emit("overlay-visibility", true)
        .map_err(|e| format!("Failed to emit visibility event: {}", e))?;

    info!("✅ Overlay shown (click-through managed by frontend)");
    Ok(())
}

/// Hide the overlay window
pub fn hide_overlay(app: &AppHandle) -> Result<(), String> {
    let window = get_overlay_window(app)
        .ok_or("Overlay window not found")?;
    
    // Emit visibility event first
    let _ = app.emit("overlay-visibility", false);
    
    // Hide the window
    window.hide()
        .map_err(|e| format!("Failed to hide overlay: {}", e))?;
    
    info!("✅ Overlay hidden");
    Ok(())
}

/// Update the overlay with the current capture region
/// 
/// This tells the overlay where to draw the border and position subtitles
pub fn update_overlay_region(app: &AppHandle, region: &CaptureRegion) -> Result<(), String> {
    let payload = OverlayRegionPayload::from(region);
    
    app.emit("overlay-update-region", payload)
        .map_err(|e| format!("Failed to emit region update: {}", e))?;
    
    debug!("📍 Overlay region updated: ({}, {}) {}x{}", 
           region.x, region.y, region.width, region.height);
    Ok(())
}

/// Check if overlay is currently visible
pub fn is_overlay_visible(app: &AppHandle) -> bool {
    get_overlay_window(app)
        .map(|w| w.is_visible().unwrap_or(false))
        .unwrap_or(false)
}

// =============================================================================
// LEGACY OVERLAY MANAGER (kept for compatibility)
// =============================================================================

/// Configuration for the overlay window
#[derive(Debug, Clone)]
pub struct OverlayPosition {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl OverlayPosition {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self { x, y, width, height }
    }
    
    pub fn from_capture_region(x: i32, y: i32, width: i32, capture_height: i32, offset_y: i32) -> Self {
        Self {
            x,
            y: y + capture_height + offset_y,
            width,
            height: 0,
        }
    }
}

pub struct OverlayManager {
    position: Option<OverlayPosition>,
    is_visible: bool,
}

impl OverlayManager {
    pub fn new() -> Self {
        Self {
            position: None,
            is_visible: false,
        }
    }
    
    pub fn set_position(&mut self, position: OverlayPosition) {
        debug!("Setting overlay position: {:?}", position);
        self.position = Some(position);
    }
    
    pub fn show(&mut self) {
        info!("Showing overlay");
        self.is_visible = true;
    }
    
    pub fn hide(&mut self) {
        info!("Hiding overlay");
        self.is_visible = false;
    }
    
    pub fn set_text(&self, text: &str) {
        debug!("Updating overlay text: {}", text);
    }
    
    pub fn is_visible(&self) -> bool {
        self.is_visible
    }
}

impl Default for OverlayManager {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_overlay_position_from_capture() {
        let pos = OverlayPosition::from_capture_region(100, 200, 800, 50, 10);
        
        assert_eq!(pos.x, 100);
        assert_eq!(pos.y, 260);
        assert_eq!(pos.width, 800);
    }
    
    #[test]
    fn test_overlay_manager() {
        let mut manager = OverlayManager::new();
        
        assert!(!manager.is_visible());
        
        manager.show();
        assert!(manager.is_visible());
        
        manager.hide();
        assert!(!manager.is_visible());
    }
}
