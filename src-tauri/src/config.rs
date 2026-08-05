// =============================================================================
// CONFIG.RS - Application Configuration
// =============================================================================
// This file defines the settings/configuration for the app.
// These settings can be saved to disk and restored when the app restarts.
// =============================================================================

pub use crate::engine_config::ManagedLocalRuntimeConfig;
use serde::{Deserialize, Serialize};

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

    /// Whether to minimize to system tray instead of closing
    pub minimize_to_tray: bool,

    /// Settings on disk this struct does not model, carried through untouched
    /// rather than dropped on save. See `config_store` for why (#64).
    #[serde(flatten, default)]
    pub unmodelled: serde_json::Map<String, serde_json::Value>,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowPreferences {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub scale_factor: Option<f64>,
    pub is_maximized: bool,
}

/// Configuration for translation backends and fallback behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TranslationConfig {
    /// Enable Foundry Local backend (primary, OpenAI-compatible)
    pub enable_foundry_local: bool,

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

    /// OCR-specific settings
    #[serde(default)]
    pub ocr: OcrConfig,
}

/// OCR-specific configuration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OcrConfig {
    /// Retained only so existing settings files still deserialize; nothing reads
    /// it. Windows OCR publishes no confidence. See `ValidationStrictness`.
    #[serde(default = "default_confidence_threshold")]
    pub confidence_threshold: f32,

    /// Enable image preprocessing before OCR (grayscale, contrast enhancement, binarization).
    /// This can improve OCR accuracy for noisy or low-contrast images.
    #[serde(default = "default_preprocessing_enabled")]
    pub preprocessing_enabled: bool,

    /// Convert image to grayscale before OCR.
    /// Recommended: true for most subtitle scenarios.
    #[serde(default = "default_grayscale")]
    pub grayscale: bool,

    /// Apply a linear contrast stretch before OCR. Not equalisation: on a
    /// subtitle strip that promotes half the background into the glyph range.
    #[serde(default = "default_contrast_enhancement")]
    pub contrast_enhancement: bool,

    /// Split glyphs from background at an automatically chosen threshold, then
    /// normalise to dark text on a light page. Recommended: true.
    #[serde(default = "default_binarize")]
    pub binarize: bool,

    /// Run OCR several times with weaker preprocessing and keep the longest
    /// read. None of those passes binarize, so this currently costs a multiple
    /// of the OCR time for a worse read than the single adaptive pass.
    #[serde(default = "default_multi_pass_enabled")]
    pub enable_multi_pass: bool,

    /// Number of OCR passes to run when multi-pass is enabled.
    #[serde(default = "default_multi_pass_count")]
    pub multi_pass_count: u32,

    /// How many significant characters a line must carry to be worth
    /// translating. See `ValidationStrictness::min_significant_chars`.
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
    /// Minimum letters and digits a recognised line needs to be worth
    /// translating. Replaced a confidence threshold that no engine could supply.
    pub fn min_significant_chars(&self) -> usize {
        match self {
            ValidationStrictness::Permissive => 1,
            ValidationStrictness::Moderate => 2,
            ValidationStrictness::Strict => 4,
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
        // Migrate legacy backend settings to the single curated engine.
        self.enable_foundry_local = true;
        self.allow_mock_fallback = false;

        // OCR config normalization
        self.ocr.confidence_threshold = self.ocr.confidence_threshold.clamp(0.0, 1.0);
        self.ocr.multi_pass_count = self.ocr.multi_pass_count.clamp(1, 5);
    }
}

impl AppConfig {
    pub fn normalize(&mut self) {
        self.translation.normalize();
        // The frontend keeps its own copy of this default and posts it back over
        // the stored settings, so 500ms kept returning after the backend moved
        // to 250ms. No UI exposes the field, so a stored 500 is that bug's
        // residue rather than a choice, and it cost up to half a second of
        // detection delay on every line.
        if self.capture_interval_ms == 500 {
            self.capture_interval_ms = Self::default().capture_interval_ms;
        }
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

    /// App-managed OpenAI-compatible endpoint. Hidden from normal-mode UI.
    pub endpoint_url: Option<String>,

    /// App-owned HY-MT runtime metadata.
    pub managed_runtime: Option<ManagedLocalRuntimeConfig>,

    /// Where the engine was installed, kept independently of `managed_runtime`.
    ///
    /// The install location used to be derivable only from `managed_runtime`,
    /// so losing that record also lost the 1.1 GB sitting on disk: setup fell
    /// back to the default cache directory and began downloading again (#65).
    /// Held separately precisely so it survives the registration.
    pub engine_cache_root: Option<String>,
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
            capture_interval_ms: 250,             // Capture every 250ms
            overlay: OverlayConfig::default(),
            translation: TranslationConfig::default(),
            last_capture_region: None,
            last_capture_scale_factor: None,
            window_preferences: WindowPreferences::default(),
            minimize_to_tray: true,
            unmodelled: serde_json::Map::new(),
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
            // Never present untranslated OCR as a successful translation.
            allow_mock_fallback: false,
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
            endpoint_url: None,
            managed_runtime: None,
            engine_cache_root: None,
        }
    }
}

// =============================================================================
// PERSISTENCE
// =============================================================================
// Reading and writing config.json lives in `config_store`, where the failures
// worth guarding against - an unreadable config mistaken for an absent one, a
// half-written file, a save that drops an engine registration still on disk -
// can be tested without a running Tauri app. Re-exported here so callers keep
// using `config::load_config` and `config::save_config`.

pub use crate::config_store::{get_config_path, load_config, save_config};

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
