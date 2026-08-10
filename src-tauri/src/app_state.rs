//! Shared application state that persists across Tauri commands.

use std::sync::{Arc, Mutex};

use tokio::sync::watch;

use crate::config::{AppConfig, CaptureRegion};
use crate::llm::TranslationDiagnosticsState;
use crate::pipeline_session::PipelineClock;
use crate::selector_window::SelectorSnapshot;
use crate::startup_gate::StartupGate;
use crate::sync_utils::lock_or_recover;

/// The application state, managed by Tauri
pub struct AppState {
    pub startup_gate: StartupGate,
    /// Current app configuration (settings)
    pub config: Mutex<AppConfig>,
    /// Whether translation is currently active
    pub is_running: Mutex<bool>,
    /// The current capture region (if set)
    pub capture_region: Mutex<Option<CaptureRegion>>,
    /// DPI scale factor for the capture region (logical -> physical)
    pub capture_scale_factor: Mutex<f64>,
    /// Stop signal sender for the translation loop
    /// When we send `true` through this, the loop stops
    pub stop_signal: Mutex<Option<watch::Sender<bool>>>,
    /// Diagnostics for translation backends
    pub translation_diagnostics: Arc<Mutex<TranslationDiagnosticsState>>,
    /// Monotonic session/capture identities used to suppress stale async results.
    pub pipeline_clock: Arc<PipelineClock>,

    /// Latest "desktop snapshot" for the area selector window.
    ///
    /// Why we need this:
    /// - On some Windows/WebView2 versions, transparent webviews regress to opaque grey/black.
    /// - The selector window is supposed to be fullscreen transparent so the user can see the desktop.
    /// - As a fallback, we capture a screenshot *before* showing the selector and render it as an image.
    pub selector_snapshot: Mutex<Option<SelectorSnapshot>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            startup_gate: StartupGate::default(),
            config: Mutex::new(AppConfig::default()),
            is_running: Mutex::new(false),
            capture_region: Mutex::new(None),
            capture_scale_factor: Mutex::new(1.0),
            stop_signal: Mutex::new(None),
            translation_diagnostics: Arc::new(Mutex::new(TranslationDiagnosticsState::default())),
            pipeline_clock: Arc::new(PipelineClock::default()),
            selector_snapshot: Mutex::new(None),
        }
    }
}

impl AppState {
    /// Record the region to capture and the DPI scale it was measured at.
    ///
    /// Work already in flight was framed against the old region, so it is
    /// invalidated here rather than being allowed to land on the overlay.
    ///
    /// Both guards are held to the end deliberately. A reader that acquired
    /// the region lock between the two writes would pair the new region with
    /// the old scale factor and capture the wrong physical pixels.
    pub fn set_capture_region(&self, region: CaptureRegion, scale_factor: f64) {
        let mut capture_region = lock_or_recover(&self.capture_region);
        *capture_region = Some(region);

        let mut capture_scale_factor = lock_or_recover(&self.capture_scale_factor);
        *capture_scale_factor = scale_factor;
        self.pipeline_clock.invalidate_capture();
    }

    /// The region currently being captured, if the user has chosen one.
    pub fn current_capture_region(&self) -> Option<CaptureRegion> {
        *lock_or_recover(&self.capture_region)
    }

    /// The DPI scale factor the current region was measured at.
    pub fn capture_scale_factor(&self) -> f64 {
        *lock_or_recover(&self.capture_scale_factor)
    }

    /// Whether the translation loop is running.
    pub fn is_translation_running(&self) -> bool {
        *lock_or_recover(&self.is_running)
    }
}

/// Reject a region that cannot be captured before it reaches the state.
///
/// A zero or negative extent produces no pixels, and a non-positive scale
/// factor maps every logical coordinate onto the same physical one.
pub fn validate_capture_region(width: i32, height: i32, scale_factor: f64) -> Result<(), String> {
    if width <= 0 || height <= 0 {
        return Err("Width and height must be positive".to_string());
    }
    if scale_factor <= 0.0 {
        return Err("Scale factor must be positive".to_string());
    }
    Ok(())
}

#[cfg(test)]
#[path = "app_state_tests.rs"]
mod tests;
