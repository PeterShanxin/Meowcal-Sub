// =============================================================================
// CONFIG.RS - Application Configuration
// =============================================================================
// This file defines the settings/configuration for the app.
// These settings can be saved to disk and restored when the app restarts.
// =============================================================================

use serde::{Deserialize, Serialize};

// =============================================================================
// APP CONFIG
// =============================================================================

/// The main configuration for the app
/// 
/// This is what gets saved/loaded from settings, and what the UI reads/writes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]  // Use camelCase in JSON (JavaScript convention)
pub struct AppConfig {
    /// The language to translate FROM (source language)
    /// Examples: "en-US", "ja-JP", "zh-CN"
    pub source_language: String,
    
    /// The language to translate TO (target language)
    /// Examples: "en-US", "zh-CN", "es-ES"
    pub target_language: String,
    
    /// How often to capture the screen (in milliseconds)
    /// Lower = more responsive, but more CPU/battery usage
    /// Recommended: 500-1000ms
    pub capture_interval_ms: u32,
    
    /// Overlay settings
    pub overlay: OverlayConfig,

    /// Translation backend settings
    #[serde(default)]
    pub translation: TranslationConfig,
    
    /// Whether to start translation automatically when app opens
    pub auto_start: bool,
    
    /// Whether to minimize to system tray instead of closing
    pub minimize_to_tray: bool,
    
    /// Whether to start with Windows
    pub start_with_windows: bool,
}

/// Configuration for the subtitle overlay appearance
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayConfig {
    /// Font size for the translated text (in pixels)
    pub font_size: u32,
    
    /// Font family (e.g., "Segoe UI", "Arial", "Microsoft YaHei")
    pub font_family: String,
    
    /// Text color as CSS color (e.g., "#FFFFFF", "white", "rgb(255,255,255)")
    pub text_color: String,
    
    /// Background color as CSS color with alpha (e.g., "rgba(0,0,0,0.7)")
    pub background_color: String,
    
    /// How much to offset the overlay below the capture region (in pixels)
    pub offset_y: i32,
    
    /// Maximum width of the overlay (in pixels, 0 = match capture region)
    pub max_width: u32,
}

/// Configuration for translation backends and fallback behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TranslationConfig {
    /// Preferred backend id (use "auto" for fallback selection)
    pub preferred_backend: String,

    /// Enable Windows AI backend (Phi Silica / LanguageModel)
    pub enable_windows_ai: bool,

    /// Enable offline MT backend (translateLocally/ORT)
    pub enable_offline_mt: bool,

    /// Enable experimental Edge Translator backend
    pub enable_edge_translator: bool,

    /// Allow passthrough/mock fallback if all backends fail
    pub allow_mock_fallback: bool,

    /// Offline MT backend configuration
    #[serde(default)]
    pub offline_mt: OfflineMtConfig,
}

/// Configuration for offline MT backend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OfflineMtConfig {
    /// Optional path to translateLocally binary
    pub binary_path: Option<String>,

    /// Translation timeout in milliseconds
    pub timeout_ms: u32,

    /// Maximum characters per chunk
    pub max_chunk_chars: usize,
}

/// A rectangular region on the screen
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CaptureRegion {
    /// X coordinate of the top-left corner
    pub x: i32,
    /// Y coordinate of the top-left corner
    pub y: i32,
    /// Width of the region
    pub width: i32,
    /// Height of the region
    pub height: i32,
}

// =============================================================================
// DEFAULT VALUES
// =============================================================================
// Rust's Default trait lets us define sensible default values for our config.

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            source_language: "en-US".to_string(),
            target_language: "zh-CN".to_string(),  // Chinese as default target
            capture_interval_ms: 500,               // Capture every 500ms
            overlay: OverlayConfig::default(),
            translation: TranslationConfig::default(),
            auto_start: false,
            minimize_to_tray: true,
            start_with_windows: false,
        }
    }
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            font_size: 24,
            font_family: "Segoe UI".to_string(),
            text_color: "#FFFFFF".to_string(),              // White text
            background_color: "rgba(0, 0, 0, 0.75)".to_string(), // Semi-transparent black
            offset_y: 10,                                    // 10px below capture region
            max_width: 0,                                    // Match capture region width
        }
    }
}

// =============================================================================
// HELPER METHODS
// =============================================================================

impl CaptureRegion {
    /// Create a new capture region
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self { x, y, width, height }
    }
    
    /// Check if the region is valid (positive dimensions)
    pub fn is_valid(&self) -> bool {
        self.width > 0 && self.height > 0
    }
    
    /// Get the area in pixels
    pub fn area(&self) -> i32 {
        self.width * self.height
    }

    /// Scale the region by a DPI scale factor (logical -> physical pixels)
    pub fn scaled(&self, scale: f64) -> Self {
        if (scale - 1.0).abs() < f64::EPSILON {
            return *self;
        }

        let scaled_x = (self.x as f64 * scale).round() as i32;
        let scaled_y = (self.y as f64 * scale).round() as i32;
        let scaled_width = (self.width as f64 * scale).round().max(1.0) as i32;
        let scaled_height = (self.height as f64 * scale).round().max(1.0) as i32;

        Self {
            x: scaled_x,
            y: scaled_y,
            width: scaled_width,
            height: scaled_height,
        }
    }
}

impl Default for TranslationConfig {
    fn default() -> Self {
        Self {
            preferred_backend: "auto".to_string(),
            enable_windows_ai: cfg!(target_os = "windows"),
            enable_offline_mt: true,
            enable_edge_translator: false,
            allow_mock_fallback: true,
            offline_mt: OfflineMtConfig::default(),
        }
    }
}

impl Default for OfflineMtConfig {
    fn default() -> Self {
        Self {
            binary_path: None,
            timeout_ms: 3000,
            max_chunk_chars: 500,
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
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.source_language, "en-US");
        assert_eq!(config.target_language, "zh-CN");
        assert_eq!(config.capture_interval_ms, 500);
    }
    
    #[test]
    fn test_capture_region_valid() {
        let region = CaptureRegion::new(0, 0, 100, 50);
        assert!(region.is_valid());
        assert_eq!(region.area(), 5000);
    }
    
    #[test]
    fn test_capture_region_invalid() {
        let region = CaptureRegion::new(0, 0, 0, 50);
        assert!(!region.is_valid());
    }
}
