//! Capture-session retries keep the same monitor through WGC recovery and GDI fallback.

use crate::capture;
use crate::capture_target::CaptureMonitor;
use crate::config::CaptureRegion;
use crate::event_payloads::CaptureStatusPayload;
use tauri::{AppHandle, Emitter};
use tracing::{info, warn};

/// Result of a capture attempt with session state
pub(crate) enum CaptureAttemptResult {
    /// Capture succeeded with the image data
    Success(capture::CaptureResult),
    /// Capture failed and should be retried after a delay
    RetryAfterDelay,
    DisplayChanged,
}

/// State for the capture session fallback logic
pub(crate) struct CaptureSessionState {
    use_persistent: bool,
    failure_count: u32,
    fallback_notified: bool,
}

impl CaptureSessionState {
    pub(crate) fn new(use_persistent: bool) -> Self {
        Self {
            use_persistent,
            failure_count: 0,
            fallback_notified: false,
        }
    }
}

/// Attempt to capture the screen region, handling session recovery and fallback.
///
/// This flattens the deeply nested capture logic into a single function.
pub(crate) fn try_capture(
    region: &CaptureRegion,
    monitor: &CaptureMonitor,
    state: &mut CaptureSessionState,
    app: &AppHandle,
) -> CaptureAttemptResult {
    if crate::display_monitor::require_present(app, monitor).is_err()
        || crate::display_monitor::require_overlay(app, monitor).is_err()
    {
        return CaptureAttemptResult::DisplayChanged;
    }
    let result = capture_checked(region, monitor, state, app);
    if crate::display_monitor::require_present(app, monitor).is_err()
        || crate::display_monitor::require_overlay(app, monitor).is_err()
    {
        return CaptureAttemptResult::DisplayChanged;
    }
    result
}

fn capture_checked(
    region: &CaptureRegion,
    monitor: &CaptureMonitor,
    state: &mut CaptureSessionState,
    app: &AppHandle,
) -> CaptureAttemptResult {
    // Try persistent session first if available
    if state.use_persistent {
        match capture::capture_with_session(monitor, region) {
            Ok(result) => {
                state.failure_count = 0;
                return CaptureAttemptResult::Success(result);
            }
            Err(capture::CaptureError::DisplayChanged) => {
                return CaptureAttemptResult::DisplayChanged
            }
            Err(e) => {
                state.failure_count += 1;
                warn!(
                    "⚠️ Session capture failed (attempt {}): {}",
                    state.failure_count, e
                );

                // Try to recover the session on first failure
                if state.failure_count == 1 {
                    capture::close_capture_session();
                    if let Err(restart_err) = capture::init_capture_session(monitor) {
                        warn!(
                            "⚠️ Failed to restart capture session, falling back: {}",
                            restart_err
                        );
                        state.use_persistent = false;
                    } else {
                        info!("✅ Capture session restarted");
                    }
                } else if state.failure_count >= 3 {
                    warn!("⚠️ Disabling persistent capture after repeated failures");
                    capture::close_capture_session();
                    state.use_persistent = false;
                }
            }
        }
    }

    // Fall back to smart_capture
    match capture::smart_capture(monitor, region) {
        Ok((result, using_fallback)) => {
            if using_fallback && !state.fallback_notified {
                state.fallback_notified = true;
                let status = CaptureStatusPayload {
                    using_fallback: true,
                    message: "Using GDI fallback - video content may not capture correctly"
                        .to_string(),
                    is_error: false,
                };
                let _ = app.emit("capture-status", status);
            }
            CaptureAttemptResult::Success(result)
        }
        Err(capture::CaptureError::DisplayChanged) => CaptureAttemptResult::DisplayChanged,
        Err(e) => {
            warn!("⚠️ Capture failed: {}", e);
            let status = CaptureStatusPayload {
                using_fallback: false,
                message: format!("Capture failed: {}", e),
                is_error: true,
            };
            let _ = app.emit("capture-status", status);
            CaptureAttemptResult::RetryAfterDelay
        }
    }
}
