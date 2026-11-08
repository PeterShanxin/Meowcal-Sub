//! Sending overlay messages to the WinUI OverlayHost.
//!
//! Premium legacy is the default. The WinUI overlay is still experimental and
//! can be opaque/black on some systems, so every message is dropped unless the
//! opt-in flag is set — the IPC server is not even started otherwise.

use std::sync::Arc;

use tauri::{AppHandle, Manager};
use tracing::warn;

use crate::env_flags::env_truthy;
use crate::ipc::{IpcMessage, IpcServer};

/// Send a message to OverlayHost via IPC
pub(crate) async fn send_overlay_message(app: &AppHandle, message: IpcMessage) {
    // Premium legacy is the default. The WinUI overlay is still experimental and can be
    // opaque/black on some systems, so keep it opt-in for now.
    if !env_truthy("MEOWCAL_USE_WINUI_OVERLAY") {
        return;
    }

    // The experimental host has no monitor-local geometry contract. Keep the
    // Tauri overlay as the sole renderer on multi-display desktops.
    if message.message_type != "Overlay.Hide"
        && crate::display_monitor::available(app).map_or(true, |monitors| monitors.len() != 1)
    {
        return;
    }

    if let Some(ipc_server) = app.try_state::<Arc<IpcServer>>() {
        ipc_server.send(message).await;
    } else {
        warn!("⚠️ IPC server not initialized, cannot send message");
    }
}
