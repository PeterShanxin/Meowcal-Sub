// =============================================================================
// OVERLAY LIVENESS - renderer readiness and heartbeat signaling
// =============================================================================
// Rust used to treat `app.emit(...)` success as a healthy overlay even when
// the WebView2 renderer had stopped consuming events, leaving the overlay
// visually stale forever (issue #112). This module gives Rust the missing
// signals:
//
//   overlay-ready      - sent once, after this page's Tauri listeners are
//                        registered. Proves the document bootstrapped.
//   overlay-heartbeat  - sent every second for the life of the page. Proves
//                        the renderer is still executing; a wedged renderer
//                        stops beating.
//
// The overlay page runs these in the same main thread that handles Tauri
// events, so heartbeat staleness is exactly the confirmed #112 wedge.
//
// Browser mode has no Tauri bridge: every function is a safe no-op there.
// Browser-mode tests cover this contract only; they do NOT prove native
// WebView2 recovery.
// =============================================================================

// Must match HEARTBEAT_INTERVAL in src-tauri/src/overlay/liveness.rs.
const HEARTBEAT_INTERVAL_MS = 1000;

let readySent = false;
let heartbeatTimer = null;

function canSignal() {
  return Boolean(window.__TAURI__?.event?.emit);
}

function safeEmit(name) {
  if (!canSignal()) return;
  window.__TAURI__.event.emit(name).catch((error) => {
    console.warn(`Failed to emit ${name}:`, error);
  });
}

// Announce that this page's Tauri listeners are registered, then start the
// heartbeat. Call exactly once, after event listener setup completes.
function signalReady() {
  if (readySent) return;
  readySent = true;
  safeEmit("overlay-ready");
  if (heartbeatTimer || !canSignal()) return;
  heartbeatTimer = setInterval(() => safeEmit("overlay-heartbeat"), HEARTBEAT_INTERVAL_MS);
}

window.OverlayLiveness = {
  HEARTBEAT_INTERVAL_MS,
  signalReady,
};
