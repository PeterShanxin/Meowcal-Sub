// =============================================================================
// OVERLAY LIVENESS - renderer heartbeat, stale detection, bounded recovery
// =============================================================================
// Rust used to equate `app.emit(...)` success and `window.show()` success with
// a healthy overlay. Both return Ok even when the WebView2 renderer has
// stopped consuming events, so a wedged overlay stayed visually stale forever
// (issue #112: native window alive, renderer alive, Tauri handlers not
// running, SetWindowRgn stuck on an old subtitle box).
//
// This module owns the distinction:
//
//   native window alive  !=  renderer alive and consuming events
//
// The overlay frontend sends `overlay-ready` once its listeners are
// registered and then `overlay-heartbeat` every second for as long as the
// page runs. A renderer whose main thread stopped processing events stops
// sending both. Rust records the last timestamps and, on the next show, runs
// exactly one bounded recovery (a single WebView reload) when the renderer
// looks stale. The show path resets the native clip to a safe empty region
// first, so a stale subtitle clip cannot hide the new session.
// =============================================================================

use std::time::{Duration, Instant};

use tauri::{AppHandle, Listener, Manager};
use tracing::{info, warn};

use crate::app_state::AppState;
use crate::sync_utils::lock_or_recover;

/// Overlay window label, matching `tauri.conf.json`.
pub const OVERLAY_WINDOW_LABEL: &str = "overlay";

/// Frontend heartbeat interval. Keep in sync with `overlay-liveness.js`.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);

/// A renderer is considered stale when its last heartbeat is older than this.
///
/// The frontend beats once a second, so three missed beats is the margin.
/// One missed beat is a busy moment, not a wedge.
pub const HEARTBEAT_STALE_AFTER: Duration = Duration::from_secs(3);

/// How long a recovery waits for the reloaded page to announce readiness
/// before giving up. Bounded: a wedged renderer cannot hold the show forever.
pub const READY_WAIT_TIMEOUT: Duration = Duration::from_secs(5);

/// Poll interval while waiting for the reloaded page to become ready.
const READY_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Renderer liveness observed from Rust.
///
/// Pure state: every method takes `now: Instant` so the recovery decision is
/// deterministic in tests without sleeping real seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LivenessState {
    /// When the overlay frontend announced its listeners were registered.
    ready_at: Option<Instant>,
    /// When the overlay frontend last proved it was still executing.
    last_heartbeat_at: Option<Instant>,
    /// When the current bounded recovery started, if one is in flight.
    recovery_started_at: Option<Instant>,
}

impl LivenessState {
    /// Record that the overlay frontend bootstrapped its event listeners.
    pub fn mark_ready(&mut self, now: Instant) {
        self.ready_at = Some(now);
    }

    /// Record that the overlay renderer is still executing.
    pub fn mark_heartbeat(&mut self, now: Instant) {
        self.last_heartbeat_at = Some(now);
    }

    /// Begin a bounded recovery. Consumed by [`Self::finish_recovery`].
    pub fn begin_recovery(&mut self, now: Instant) {
        self.recovery_started_at = Some(now);
    }

    /// End the current recovery, whether it succeeded or timed out.
    ///
    /// Recovery is one-shot per show: after this, nothing in the state can
    /// loop or retry until the next show calls `begin_recovery` again.
    pub fn finish_recovery(&mut self) {
        self.recovery_started_at = None;
    }

    /// Whether the frontend ever announced its listeners were registered.
    pub fn is_ready(&self) -> bool {
        self.ready_at.is_some()
    }

    /// Whether the renderer looks wedged: it never bootstrapped, or its last
    /// heartbeat is older than `stale_after`.
    pub fn is_stale(&self, now: Instant, stale_after: Duration) -> bool {
        match self.last_heartbeat_at {
            Some(last) => now.saturating_duration_since(last) > stale_after,
            None => true,
        }
    }

    /// Whether a ready event arrived after the current recovery began.
    ///
    /// The pre-recovery bootstrap timestamp must not count: only the reloaded
    /// page's readiness proves the reload produced a live listener.
    pub fn ready_since_recovery_start(&self) -> bool {
        match (self.ready_at, self.recovery_started_at) {
            (Some(ready), Some(started)) => ready >= started,
            _ => false,
        }
    }
}

/// Whether a show must recover the overlay before trusting it.
///
/// A renderer that never bootstrapped cannot process events at all; a renderer
/// that stopped heartbeating is the confirmed #112 wedge.
pub fn needs_recovery(state: &LivenessState, now: Instant, stale_after: Duration) -> bool {
    !state.is_ready() || state.is_stale(now, stale_after)
}

/// Register the Rust-side listeners for the frontend liveness events.
///
/// Called once from the composition root. Both events are fire-and-forget
/// timestamp updates; heartbeat events never log, because they arrive once a
/// second for the life of the app.
pub fn register_listeners<R: tauri::Runtime>(app: &AppHandle<R>) {
    let ready_handle = app.clone();
    app.listen("overlay-ready", move |_event| {
        let state = ready_handle.state::<AppState>();
        lock_or_recover(&state.overlay_liveness).mark_ready(Instant::now());
    });

    let heartbeat_handle = app.clone();
    app.listen("overlay-heartbeat", move |_event| {
        let state = heartbeat_handle.state::<AppState>();
        lock_or_recover(&state.overlay_liveness).mark_heartbeat(Instant::now());
    });
}

/// Recover a stale overlay renderer, bounded.
///
/// Runs only when the renderer looks stale. Steps:
///
/// 1. reload the overlay WebView once;
/// 2. wait up to [`READY_WAIT_TIMEOUT`] for `overlay-ready` from the reloaded
///    page;
/// 3. log the outcome explicitly.
///
/// The show path (`show_overlay`) has already reset the native region to a
/// safe empty state before this runs, so no stale clip can survive into the
/// new session while the reload happens.
///
/// There is exactly one reload attempt per show. A timeout does not loop: the
/// overlay is left with the safe empty clip (nothing stale is displayed) and
/// the failure is logged; the next show may try again.
pub async fn recover_overlay(app: &AppHandle) {
    let Some(window) = app.get_webview_window(OVERLAY_WINDOW_LABEL) else {
        warn!("Overlay window not found; recovery skipped");
        return;
    };

    let now = Instant::now();
    let needs = {
        let state = app.state::<AppState>();
        let liveness = lock_or_recover(&state.overlay_liveness);
        needs_recovery(&liveness, now, HEARTBEAT_STALE_AFTER)
    };
    if !needs {
        return;
    }

    info!("Overlay renderer stale or not ready; starting bounded recovery");

    {
        let state = app.state::<AppState>();
        let mut liveness = lock_or_recover(&state.overlay_liveness);
        liveness.begin_recovery(now);
    }

    if let Err(error) = window.reload() {
        warn!("Overlay webview reload failed: {}", error);
    }

    let deadline = now + READY_WAIT_TIMEOUT;
    let mut recovered = false;
    while Instant::now() < deadline {
        tokio::time::sleep(READY_POLL_INTERVAL).await;
        let ready = {
            let state = app.state::<AppState>();
            let liveness = lock_or_recover(&state.overlay_liveness);
            liveness.ready_since_recovery_start()
        };
        if ready {
            recovered = true;
            break;
        }
    }

    {
        let state = app.state::<AppState>();
        let mut liveness = lock_or_recover(&state.overlay_liveness);
        liveness.finish_recovery();
    }

    if recovered {
        info!("Overlay renderer recovered after reload");
    } else {
        warn!(
            "Overlay recovery timed out after {:?}; clip reset to a safe empty region, \
             the overlay may stay hidden until the next start",
            READY_WAIT_TIMEOUT
        );
    }
}

#[cfg(test)]
#[path = "liveness_tests.rs"]
mod tests;
