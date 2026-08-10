//! The capture-area selector window and its desktop-snapshot background.
//!
//! Two selectors exist. The WinUI OverlayHost one is an opt-in experiment; the
//! legacy webview one is the default because it has a stable background and
//! correct DPI mapping. The background is a screenshot rather than real
//! transparency: on some Windows/WebView2 versions a transparent webview
//! regresses to opaque grey, which would hide the desktop the user is
//! selecting from.

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::{async_runtime, AppHandle, Emitter, Manager};
use tracing::{info, warn};

use crate::app_state::AppState;
use crate::capture::{self, CaptureResult};
use crate::config::CaptureRegion;
use crate::env_flags::env_truthy;
use crate::ipc::{IpcMessage, IpcServer};
use crate::sync_utils::lock_or_recover;

/// A full-screen "desktop snapshot" for the area selector background.
///
/// This is a workaround for transparency regressions: instead of relying on the webview
/// to be truly transparent, we render a screenshot behind the selection UI.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectorSnapshot {
    /// `data:image/png;base64,...`
    pub data_url: String,
    /// Snapshot width in physical pixels.
    pub width: i32,
    /// Snapshot height in physical pixels.
    pub height: i32,
}

/// Result from `open` so the UI can show whether we used WinUI OverlayHost
/// or fell back to the legacy webview selector.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AreaSelectorMode {
    Winui,
    Legacy,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAreaSelectorResult {
    pub mode: AreaSelectorMode,
}

/// Open the area selector overlay window.
pub async fn open(app: AppHandle, state: &AppState) -> Result<OpenAreaSelectorResult, String> {
    // We currently prefer the "legacy" webview-based selector because it has a stable
    // desktop-snapshot background and correct DPI mapping.
    //
    // WinUI selector is kept as an opt-in experiment for future work.
    let prefer_winui = env_truthy("MEOWCAL_USE_WINUI_SELECTOR");

    if prefer_winui {
        info!("🎯 Opening area selector via OverlayHost (opt-in)");

        if let Some(ipc_server) = app.try_state::<Arc<IpcServer>>() {
            // On startup, the WinUI OverlayHost can take a moment to launch and connect.
            // Wait briefly so the first click is more likely to use WinUI instead of the legacy UI.
            const WAIT_MS: u64 = 2500;
            const STEP_MS: u64 = 50;
            let message = IpcMessage::new("Region.RequestOpenSelector");

            let mut waited = 0u64;
            while waited <= WAIT_MS {
                if ipc_server.is_connected() && ipc_server.send(message.clone()).await {
                    return Ok(OpenAreaSelectorResult {
                        mode: AreaSelectorMode::Winui,
                    });
                }

                tokio::time::sleep(Duration::from_millis(STEP_MS)).await;
                waited = waited.saturating_add(STEP_MS);
            }

            warn!("⚠️ OverlayHost not connected; falling back to legacy selector");
        } else {
            warn!("⚠️ IPC server not initialized; falling back to legacy selector");
        }
    }

    open_legacy(app, state).await?;
    Ok(OpenAreaSelectorResult {
        mode: AreaSelectorMode::Legacy,
    })
}

/// Legacy area selector (kept for fallback if WinUI3 is not available)
async fn open_legacy(app: AppHandle, state: &AppState) -> Result<(), String> {
    info!("Opening area selector...");

    let Some(window) = app.get_webview_window("selector") else {
        return Err("Selector window not found".to_string());
    };

    // Capture a background snapshot BEFORE showing the selector window.
    // If we capture after showing, the screenshot will include the selector UI itself.
    //
    // This is best-effort: if capture fails, we still show the selector (it will just be grey).
    match async_runtime::spawn_blocking(capture_desktop_snapshot).await {
        Ok(Ok((snapshot, used_fallback))) => {
            if used_fallback {
                warn!("Area selector snapshot: Graphics Capture failed, used GDI fallback");
            } else {
                info!("Area selector snapshot: captured via Graphics Capture");
            }

            // Store for the selector window to pull on load (and for subsequent opens).
            *lock_or_recover(&state.selector_snapshot) = Some(snapshot.clone());

            // Also push it as an event (helps if the selector window is already loaded).
            let _ = window.emit("selector-background-snapshot", snapshot);
        }
        Ok(Err(e)) => {
            warn!("Area selector snapshot capture failed: {}", e);
            *lock_or_recover(&state.selector_snapshot) = None;
        }
        Err(join_err) => {
            warn!("Area selector snapshot task failed: {}", join_err);
            *lock_or_recover(&state.selector_snapshot) = None;
        }
    }

    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    info!("✅ Area selector opened!");

    Ok(())
}

/// Screenshot the primary screen for use as the selector background.
///
/// Returns the snapshot and whether the GDI fallback produced it.
fn capture_desktop_snapshot() -> Result<(SelectorSnapshot, bool), String> {
    // Capture the primary screen in physical pixels.
    let (width, height) = capture::get_screen_dimensions();
    if width <= 0 || height <= 0 {
        return Err(format!("Invalid screen dimensions: {}x{}", width, height));
    }

    let region = CaptureRegion::new(0, 0, width, height);
    let (capture, used_fallback) = capture::smart_capture(&region).map_err(|e| format!("{}", e))?;

    Ok((encode_snapshot(capture, width, height)?, used_fallback))
}

/// Turn a captured frame into the `data:` URL the selector webview renders.
fn encode_snapshot(
    capture: CaptureResult,
    width: i32,
    height: i32,
) -> Result<SelectorSnapshot, String> {
    use base64::Engine;

    // Convert BGRA -> RGBA (swap red/blue channels).
    // Our capture backends return BGRA to match Windows APIs.
    let mut rgba = capture.data;
    for px in rgba.chunks_exact_mut(4) {
        px.swap(0, 2);
    }

    // Encode to PNG.
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, capture.width, capture.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);

        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("PNG header write failed: {}", e))?;

        writer
            .write_image_data(&rgba)
            .map_err(|e| format!("PNG encoding failed: {}", e))?;
    }

    // Base64 encode for the webview (<img src="data:...">).
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);

    Ok(SelectorSnapshot {
        data_url: format!("data:image/png;base64,{}", b64),
        width,
        height,
    })
}

/// The most recent selector background snapshot, if one is held.
pub fn snapshot(state: &AppState) -> Option<SelectorSnapshot> {
    lock_or_recover(&state.selector_snapshot).clone()
}

/// Close the area selector overlay window.
pub fn close(app: &AppHandle, state: &AppState) -> Result<(), String> {
    info!("Closing area selector...");

    let Some(window) = app.get_webview_window("selector") else {
        return Err("Selector window not found".to_string());
    };

    window.hide().map_err(|e| e.to_string())?;
    // Drop the snapshot to avoid holding a huge base64 string in memory.
    *lock_or_recover(&state.selector_snapshot) = None;
    info!("✅ Area selector closed!");

    Ok(())
}

#[cfg(test)]
#[path = "selector_window_tests.rs"]
mod tests;
