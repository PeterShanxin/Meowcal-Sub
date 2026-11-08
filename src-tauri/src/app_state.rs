//! Shared application state that persists across Tauri commands.

use std::sync::{Arc, Mutex};

use tokio::sync::watch;

use crate::capture_target::{CaptureSelection, CaptureState};
use crate::config::{AppConfig, CaptureRegion};
use crate::llm::TranslationDiagnosticsState;
use crate::pipeline_session::PipelineClock;
use crate::selector_window::SnapshotSlot;
use crate::startup_gate::StartupGate;
use crate::sync_utils::lock_or_recover;

/// The application state, managed by Tauri
pub struct AppState {
    pub startup_gate: StartupGate,
    /// Current app configuration (settings)
    pub config: Mutex<AppConfig>,
    /// Whether translation is currently active
    pub is_running: Mutex<bool>,
    pub session_lifecycle: crate::session_lifecycle::SessionLifecycle,
    pub topology_watch: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    /// Region, monitor identity, DPI and pending selector share one lock.
    pub capture: Mutex<CaptureState>,
    /// Stop signal sender for the translation loop
    /// When we send `true` through this, the loop stops
    pub stop_signal: Mutex<Option<watch::Sender<bool>>>,
    /// Diagnostics for translation backends
    pub translation_diagnostics: Arc<Mutex<TranslationDiagnosticsState>>,
    /// Monotonic session/capture identities used to suppress stale async results.
    pub pipeline_clock: Arc<PipelineClock>,

    /// Overlay renderer liveness: ready/heartbeat timestamps and recovery
    /// state, owned by `overlay::liveness`.
    pub overlay_liveness: Mutex<crate::overlay::liveness::LivenessState>,

    /// Latest "desktop snapshot" for the area selector window.
    ///
    /// Why we need this:
    /// - On some Windows/WebView2 versions, transparent webviews regress to opaque grey/black.
    /// - The selector window is supposed to be fullscreen transparent so the user can see the desktop.
    /// - As a fallback, we capture a screenshot *before* showing the selector and render it as an image.
    pub selector_snapshot: SnapshotSlot,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            startup_gate: StartupGate::default(),
            config: Mutex::new(AppConfig::default()),
            is_running: Mutex::new(false),
            session_lifecycle: crate::session_lifecycle::SessionLifecycle::default(),
            topology_watch: Mutex::new(None),
            capture: Mutex::new(CaptureState::default()),
            stop_signal: Mutex::new(None),
            translation_diagnostics: Arc::new(Mutex::new(TranslationDiagnosticsState::default())),
            pipeline_clock: Arc::new(PipelineClock::default()),
            overlay_liveness: Mutex::new(crate::overlay::liveness::LivenessState::default()),
            selector_snapshot: Mutex::new(None),
        }
    }
}

impl AppState {
    /// The settings loaded from disk. The WebView can call commands before
    /// Tauri setup has read them, and the in-memory defaults would report an
    /// installed engine as missing.
    pub async fn loaded_config(&self) -> Result<AppConfig, String> {
        self.startup_gate.wait_until_ready().await?;
        Ok(lock_or_recover(&self.config).clone())
    }

    pub fn set_capture_selection(&self, selection: CaptureSelection) {
        let mut capture = lock_or_recover(&self.capture);
        capture.selected = Some(selection);
        self.pipeline_clock.invalidate_capture();
    }

    pub fn current_capture_selection(&self) -> Option<CaptureSelection> {
        lock_or_recover(&self.capture).selected.clone()
    }

    /// Caller holds the session transition lock. A delayed display probe must
    /// never invalidate a replacement session.
    pub fn lose_capture_display(&self, session_id: u64) -> bool {
        if !self.pipeline_clock.is_session_current(session_id) {
            return false;
        }
        self.pipeline_clock.invalidate_session();
        let mut capture = lock_or_recover(&self.capture);
        capture.selected = None;
        capture.cancel_selection();
        if let Some(stop) = lock_or_recover(&self.stop_signal).as_ref() {
            let _ = stop.send(true);
        }
        true
    }

    /// The region currently being captured, if the user has chosen one.
    pub fn current_capture_region(&self) -> Option<CaptureRegion> {
        self.current_capture_selection()
            .map(|selection| selection.region)
    }

    /// The DPI scale factor the current region was measured at.
    pub fn capture_scale_factor(&self) -> f64 {
        self.current_capture_selection()
            .map_or(1.0, |selection| selection.monitor.scale_factor)
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
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        return Err("Scale factor must be positive".to_string());
    }
    Ok(())
}

#[cfg(test)]
#[path = "app_state_tests.rs"]
mod tests;
