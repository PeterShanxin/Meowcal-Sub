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
    if let Err(e) = window.set_title("") {
        info!("⚠️ Failed to clear overlay title: {}", e);
    }
    if let Err(e) = window.set_focusable(false) {
        info!("⚠️ Failed to set overlay focusable: {}", e);
    }

    // Show the window
    window.show()
        .map_err(|e| format!("Failed to show overlay: {}", e))?;

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
