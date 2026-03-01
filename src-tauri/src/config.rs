// =============================================================================
// CONFIG.RS - Application Configuration
// =============================================================================
// This file defines the settings/configuration for the app.
// These settings can be saved to disk and restored when the app restarts.
// =============================================================================

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

// =============================================================================
// APP CONFIG
// =============================================================================

/// The main configuration for the app
///
/// This is what gets saved/loaded from settings, and what the UI reads/writes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")] // Use camelCase in JSON (JavaScript convention)
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

    /// Last selected capture region
    #[serde(default)]
    pub last_capture_region: Option<CaptureRegion>,

    /// Last known DPI scale factor for the capture region (logical -> physical pixels).
    ///
    /// This is persisted so restored regions behave correctly on high-DPI displays.
    #[serde(default)]
    pub last_capture_scale_factor: Option<f64>,

    /// Window size/position preferences
    #[serde(default)]
    pub window_preferences: WindowPreferences,

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

    /// Whether to show the diagnostics overlay (debug info panel)
    #[serde(default)]
    pub show_diagnostics: bool,
}

/// Window size/position preferences
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowPreferences {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub is_maximized: bool,
}

/// Configuration for translation backends and fallback behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TranslationConfig {
    /// Enable Foundry Local backend (primary, OpenAI-compatible)
    pub enable_foundry_local: bool,

    /// Enable Windows AI backend (Phi Silica / LanguageModel)
    pub enable_windows_ai: bool,

    /// Enable offline MT backend (translateLocally/ORT)
    pub enable_offline_mt: bool,

    /// Allow passthrough/mock fallback if all backends fail
    pub allow_mock_fallback: bool,

    /// Enable context-aware translation (session memory for names, genre, history)
    pub enable_context_aware: bool,

    /// Context level for Foundry Local prompts.
    #[serde(default)]
    pub context_level: ContextLevel,

    /// How many recent translations to include in the prompt (MemoryAndRecent only).
    #[serde(default = "default_context_recent_count")]
    pub context_recent_count: usize,

    /// Context budget as a percentage of the model context window.
    ///
    /// Example: 15 = use 15% of the model context for session context.
    #[serde(default = "default_context_budget_percent")]
    pub context_budget_percent: u8,

    /// Minimum time between context summarization runs (milliseconds).
    #[serde(default = "default_context_summary_cooldown_ms")]
    pub context_summary_cooldown_ms: u32,

    /// Hard cap for subtitle source text length (characters) sent to LLM prompt builder.
    /// Keeps latency stable and prevents giant OCR blobs from spiking tokens.
    #[serde(default = "default_prompt_max_source_chars")]
    pub prompt_max_source_chars: usize,

    /// Hard cap for context text length (characters) used for context-aware translation.
    /// Newest lines are kept when trimming.
    #[serde(default = "default_prompt_max_context_chars")]
    pub prompt_max_context_chars: usize,

    /// Rolling buffer size (number of OCR lines) kept for subtitle context.
    #[serde(default = "default_context_buffer_size")]
    pub context_buffer_size: usize,

    /// Clear subtitle context after a long gap (ms) to reduce drift.
    /// Set to 0 to disable gap-based resets.
    #[serde(default = "default_context_reset_gap_ms")]
    pub context_reset_gap_ms: u32,

    /// Foundry Local backend configuration
    #[serde(default)]
    pub foundry_local: FoundryLocalConfig,

    /// Offline MT backend configuration
    #[serde(default)]
    pub offline_mt: OfflineMtConfig,

    /// OCR-specific settings
    #[serde(default)]
    pub ocr: OcrConfig,
}

/// OCR-specific configuration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OcrConfig {
    /// Minimum confidence threshold (0.0 to 1.0) for OCR text to be accepted.
    /// Text with confidence below this threshold will be skipped.
    /// Note: Windows OCR doesn't provide native confidence, so this uses heuristic scoring.
    #[serde(default = "default_confidence_threshold")]
    pub confidence_threshold: f32,

    /// Enable image preprocessing before OCR (grayscale, contrast enhancement).
    /// This can improve OCR accuracy for noisy or low-contrast images.
    #[serde(default = "default_preprocessing_enabled")]
    pub preprocessing_enabled: bool,

    /// Convert image to grayscale before OCR.
    /// Recommended: true for most subtitle scenarios.
    #[serde(default = "default_grayscale")]
    pub grayscale: bool,

    /// Apply contrast enhancement (histogram equalization) before OCR.
    /// Helps with low-contrast images or uneven lighting.
    #[serde(default = "default_contrast_enhancement")]
    pub contrast_enhancement: bool,

    /// Apply binary threshold after contrast enhancement.
    /// Converts the image to pure black and white at the midpoint (128/255).
    /// Recommended: true — reduces noise and improves OCR accuracy on subtitle text.
    #[serde(default = "default_binarize")]
    pub binarize: bool,

    /// Enable multi-pass OCR for improved accuracy.
    /// When enabled, runs OCR multiple times with different preprocessing settings
    /// and selects the result with the highest confidence.
    /// Note: This doubles/triples OCR processing time but can improve accuracy.
    #[serde(default = "default_multi_pass_enabled")]
    pub enable_multi_pass: bool,

    /// Number of OCR passes to run when multi-pass is enabled.
    /// Higher values may improve accuracy but take longer.
    /// Recommended: 2-3 passes.
    #[serde(default = "default_multi_pass_count")]
    pub multi_pass_count: u32,

    /// Validation strictness for ML-based text filtering.
    /// Controls how aggressively to filter OCR artifacts and garbage text.
    /// - Permissive: Only rejects obvious garbage (low confidence < 0.2)
    /// - Moderate: Balances false positives/negatives (rejects confidence < 0.4)
    /// - Strict: Aggressively filters potential garbage (rejects confidence < 0.6)
    #[serde(default)]
    pub validation_strictness: ValidationStrictness,
}

fn default_confidence_threshold() -> f32 {
    0.5
}

fn default_preprocessing_enabled() -> bool {
    true
}

fn default_grayscale() -> bool {
    true
}

fn default_contrast_enhancement() -> bool {
    true
}

fn default_binarize() -> bool {
    true
}

fn default_multi_pass_enabled() -> bool {
    false
}

fn default_multi_pass_count() -> u32 {
    2
}

/// Validation strictness level for ML-based text validation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum ValidationStrictness {
    /// Permissive: only rejects obvious garbage
    Permissive,
    /// Moderate: balances false positives and false negatives
    #[default]
    Moderate,
    /// Strict: aggressively filters potential garbage
    Strict,
}

impl ValidationStrictness {
    /// Get the confidence threshold for this strictness level.
    /// Returns the minimum confidence score required for OCR text to be accepted.
    pub fn threshold(&self) -> f32 {
        match self {
            ValidationStrictness::Permissive => 0.2,
            ValidationStrictness::Moderate => 0.4,
            ValidationStrictness::Strict => 0.6,
        }
    }
}

/// Context-aware prompt level for Foundry Local.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum ContextLevel {
    Off,
    MemoryOnly,
    #[default]
    MemoryAndRecent,
}

fn default_context_recent_count() -> usize {
    3
}

fn default_context_budget_percent() -> u8 {
    15
}

fn default_context_summary_cooldown_ms() -> u32 {
    5_000
}

fn default_prompt_max_source_chars() -> usize {
    300
}

fn default_prompt_max_context_chars() -> usize {
    600
}

fn default_context_buffer_size() -> usize {
    12
}

fn default_context_reset_gap_ms() -> u32 {
    6_000
}

impl TranslationConfig {
    pub fn normalize(&mut self) {
        // Keep the legacy toggle and the new level consistent.
        if !self.enable_context_aware {
            self.context_level = ContextLevel::Off;
        } else if self.context_level == ContextLevel::Off {
            self.enable_context_aware = false;
        }

        self.context_buffer_size = self.context_buffer_size.clamp(1, 50);
        self.context_recent_count = self.context_recent_count.clamp(0, self.context_buffer_size);
        self.context_budget_percent = self.context_budget_percent.clamp(5, 30);
        self.context_summary_cooldown_ms = self.context_summary_cooldown_ms.clamp(0, 120_000);
        self.prompt_max_source_chars = self.prompt_max_source_chars.clamp(50, 2_000);
        self.prompt_max_context_chars = self.prompt_max_context_chars.clamp(0, 5_000);
        self.context_reset_gap_ms = self.context_reset_gap_ms.clamp(0, 120_000);

        // Foundry Local can take a while to warm up (especially NPU models). Keep a sane
        // minimum timeout even if an older config had a too-aggressive default.
        self.foundry_local.timeout_ms = self.foundry_local.timeout_ms.clamp(2_000, 120_000);
        if self.foundry_local.timeout_ms < 15_000 {
            self.foundry_local.timeout_ms = 30_000;
        }

        self.offline_mt.timeout_ms = self.offline_mt.timeout_ms.clamp(1, 120_000);

        // OCR config normalization
        self.ocr.confidence_threshold = self.ocr.confidence_threshold.clamp(0.0, 1.0);
        self.ocr.multi_pass_count = self.ocr.multi_pass_count.clamp(1, 5);
    }
}

impl AppConfig {
    pub fn normalize(&mut self) {
        self.translation.normalize();
    }
}

/// Configuration for Foundry Local backend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FoundryLocalConfig {
    /// Selected model alias (e.g., "qwen2.5-0.5b", "phi-3-mini-4k")
    pub model: Option<String>,

    /// Request timeout in milliseconds
    pub timeout_ms: u32,
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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
            target_language: "zh-CN".to_string(), // Chinese as default target
            capture_interval_ms: 500,             // Capture every 500ms
            overlay: OverlayConfig::default(),
            translation: TranslationConfig::default(),
            last_capture_region: None,
            last_capture_scale_factor: None,
            window_preferences: WindowPreferences::default(),
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
            text_color: "#FFFFFF".to_string(), // White text
            background_color: "rgba(0, 0, 0, 0.75)".to_string(), // Semi-transparent black
            offset_y: 10,                      // 10px below capture region
            max_width: 0,                      // Match capture region width
            show_diagnostics: false,           // Off by default
        }
    }
}

// =============================================================================
// HELPER METHODS
// =============================================================================

impl CaptureRegion {
    /// Create a new capture region
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
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

    /// Returns true if this region overlaps the origin-based bounds rectangle (0..width, 0..height).
    pub fn intersects_origin_bounds(&self, bounds_width: i32, bounds_height: i32) -> bool {
        if !self.is_valid() || bounds_width <= 0 || bounds_height <= 0 {
            return false;
        }

        let left = self.x as i64;
        let top = self.y as i64;
        let right = left + self.width as i64;
        let bottom = top + self.height as i64;

        let bounds_right = bounds_width as i64;
        let bounds_bottom = bounds_height as i64;

        let inter_left = left.max(0);
        let inter_top = top.max(0);
        let inter_right = right.min(bounds_right);
        let inter_bottom = bottom.min(bounds_bottom);

        inter_right > inter_left && inter_bottom > inter_top
    }

    /// Clamp this region to fit within origin-based bounds (0..width, 0..height).
    ///
    /// The clamping behavior preserves the current width/height whenever possible by shifting the
    /// region back into view. If the region is larger than the bounds, it will be capped.
    pub fn clamp_to_bounds(&self, bounds_width: i32, bounds_height: i32) -> Option<Self> {
        if !self.is_valid() || bounds_width <= 0 || bounds_height <= 0 {
            return None;
        }

        let mut width = self.width;
        let mut height = self.height;
        let mut x = self.x;
        let mut y = self.y;

        // Cap the region size to the available bounds.
        if width > bounds_width {
            width = bounds_width;
            x = 0;
        }
        if height > bounds_height {
            height = bounds_height;
            y = 0;
        }

        // Shift into bounds while preserving size.
        if x < 0 {
            x = 0;
        }
        if y < 0 {
            y = 0;
        }

        // Ensure right/bottom edge doesn't exceed bounds.
        if (x as i64) + (width as i64) > (bounds_width as i64) {
            x = bounds_width - width;
        }
        if (y as i64) + (height as i64) > (bounds_height as i64) {
            y = bounds_height - height;
        }

        // Final sanity check.
        if width <= 0 || height <= 0 || x < 0 || y < 0 {
            return None;
        }

        Some(Self {
            x,
            y,
            width,
            height,
        })
    }
}

impl Default for TranslationConfig {
    fn default() -> Self {
        Self {
            enable_foundry_local: true,
            enable_windows_ai: cfg!(target_os = "windows"),
            enable_offline_mt: true,
            allow_mock_fallback: true,
            enable_context_aware: true, // Enabled by default
            context_level: ContextLevel::MemoryAndRecent,
            context_recent_count: default_context_recent_count(),
            context_budget_percent: default_context_budget_percent(),
            context_summary_cooldown_ms: default_context_summary_cooldown_ms(),
            prompt_max_source_chars: default_prompt_max_source_chars(),
            prompt_max_context_chars: default_prompt_max_context_chars(),
            context_buffer_size: default_context_buffer_size(),
            context_reset_gap_ms: default_context_reset_gap_ms(),
            foundry_local: FoundryLocalConfig::default(),
            offline_mt: OfflineMtConfig::default(),
            ocr: OcrConfig::default(),
        }
    }
}

impl Default for OcrConfig {
    fn default() -> Self {
        Self {
            confidence_threshold: default_confidence_threshold(),
            preprocessing_enabled: default_preprocessing_enabled(),
            grayscale: default_grayscale(),
            contrast_enhancement: default_contrast_enhancement(),
            binarize: default_binarize(),
            enable_multi_pass: default_multi_pass_enabled(),
            multi_pass_count: default_multi_pass_count(),
            validation_strictness: ValidationStrictness::default(),
        }
    }
}

impl Default for FoundryLocalConfig {
    fn default() -> Self {
        Self {
            model: None,
            timeout_ms: 30_000,
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
// PERSISTENCE
// =============================================================================

/// Get the config.json path in the app data directory.
pub fn get_config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {}", e))?;
    fs::create_dir_all(&app_dir).map_err(|e| format!("Failed to create app data dir: {}", e))?;
    Ok(app_dir.join("config.json"))
}

/// Load config from disk (fall back to defaults on error).
pub fn load_config(app: &tauri::AppHandle) -> AppConfig {
    let path = match get_config_path(app) {
        Ok(path) => path,
        Err(_) => return AppConfig::default(),
    };

    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(mut config) = serde_json::from_str::<AppConfig>(&content) {
            config.normalize();
            return config;
        }
    }

    AppConfig::default()
}

/// Save config to disk.
pub fn save_config(app: &tauri::AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = get_config_path(app)?;
    let mut config = config.clone();
    config.normalize();
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())?;
    Ok(())
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

    #[test]
    fn test_translation_config_defaults() {
        let config = TranslationConfig::default();
        assert!(config.enable_foundry_local);
        assert!(config.enable_offline_mt);
        assert!(config.allow_mock_fallback);
        assert!(config.enable_context_aware); // Context-aware enabled by default
        assert_eq!(config.context_level, ContextLevel::MemoryAndRecent);
        assert_eq!(config.context_recent_count, 3);
        assert_eq!(config.context_budget_percent, 15);
        assert_eq!(config.context_summary_cooldown_ms, 5_000);
        assert_eq!(config.prompt_max_source_chars, 300);
        assert_eq!(config.prompt_max_context_chars, 600);
        assert_eq!(config.context_buffer_size, 12);
        assert_eq!(config.context_reset_gap_ms, 6_000);
    }

    #[test]
    fn test_translation_config_serialization() {
        // Test that enable_context_aware serializes/deserializes correctly
        let config = TranslationConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("enableContextAware")); // camelCase in JSON
        assert!(json.contains("contextLevel"));

        let deserialized: TranslationConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.enable_context_aware,
            config.enable_context_aware
        );
        assert_eq!(deserialized.context_level, config.context_level);
    }

    #[test]
    fn test_translation_config_missing_field_uses_default() {
        // Test that missing enable_context_aware field uses default (true)
        let json = r#"{
            "enableFoundryLocal": true,
            "enableWindowsAi": false,
            "enableOfflineMt": true,
            "allowMockFallback": true,
            "foundryLocal": {},
            "offlineMt": {}
        }"#;
        let config: TranslationConfig = serde_json::from_str(json).unwrap();
        assert!(config.enable_context_aware); // Should default to true
    }
}
