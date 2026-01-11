// =============================================================================
// OVERLAY MODULE - Floating Subtitle Window
// =============================================================================
// This module manages the floating overlay window that displays translations.
// 
// The overlay is a separate transparent window that:
// - Floats above other windows (always on top)
// - Is click-through (doesn't interfere with clicking)
// - Positioned relative to the capture region
// =============================================================================

use tracing::{debug, info};

/// Configuration for the overlay window
#[derive(Debug, Clone)]
pub struct OverlayPosition {
    /// X coordinate on screen
    pub x: i32,
    /// Y coordinate on screen
    pub y: i32,
    /// Width of the overlay
    pub width: i32,
    /// Height of the overlay (auto if 0)
    pub height: i32,
}

impl OverlayPosition {
    /// Create a new overlay position
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self { x, y, width, height }
    }
    
    /// Calculate overlay position based on capture region
    /// 
    /// Places the overlay just below the capture region
    pub fn from_capture_region(x: i32, y: i32, width: i32, capture_height: i32, offset_y: i32) -> Self {
        Self {
            x,
            y: y + capture_height + offset_y,  // Below the capture region
            width,
            height: 0,  // Auto-height based on content
        }
    }
}

/// The overlay manager
/// 
/// Handles creation and management of the floating subtitle overlay.
/// In Tauri, we use a separate WebView window for the overlay.
pub struct OverlayManager {
    /// Current position
    position: Option<OverlayPosition>,
    /// Whether the overlay is visible
    is_visible: bool,
}

impl OverlayManager {
    /// Create a new overlay manager
    pub fn new() -> Self {
        Self {
            position: None,
            is_visible: false,
        }
    }
    
    /// Set the overlay position
    pub fn set_position(&mut self, position: OverlayPosition) {
        debug!("Setting overlay position: {:?}", position);
        self.position = Some(position);
    }
    
    /// Show the overlay
    pub fn show(&mut self) {
        info!("Showing overlay");
        self.is_visible = true;
        // TODO: Tell the overlay window to show via Tauri events
    }
    
    /// Hide the overlay
    pub fn hide(&mut self) {
        info!("Hiding overlay");
        self.is_visible = false;
        // TODO: Tell the overlay window to hide via Tauri events
    }
    
    /// Update the overlay text
    pub fn set_text(&self, text: &str) {
        debug!("Updating overlay text: {}", text);
        // TODO: Send text to overlay window via Tauri events
    }
    
    /// Check if overlay is currently visible
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
        assert_eq!(pos.y, 260);  // 200 + 50 + 10
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
