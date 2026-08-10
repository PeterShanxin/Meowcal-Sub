//! Receiving IPC messages from the WinUI OverlayHost.
//!
//! The send side lives in `overlay_ipc`; this module is the handler that turns
//! OverlayHost messages into application state changes. The OverlayHost is
//! opt-in, so this handler only ever runs with `MEOWCAL_USE_WINUI_*` enabled.

use tauri::{AppHandle, Manager};
use tracing::{info, warn};

use crate::app_state::AppState;
use crate::config::CaptureRegion;
use crate::sync_utils::lock_or_recover;

use super::protocol::{IpcMessage, SelectorResultPayload};

/// Handle a single IPC message from OverlayHost.
pub fn handle_ipc_message(app: &AppHandle, message: IpcMessage) {
    info!("📨 IPC message received: {}", message.message_type);

    match message.message_type.as_str() {
        "Selector.Result" => {
            // Parse selector result and update capture region
            if let Some(payload) = message.payload {
                if let Ok(result) = serde_json::from_value::<SelectorResultPayload>(payload) {
                    info!(
                        "✅ Selector result: ({},{}) {}x{} @ {}% DPI",
                        result.region_physical.x,
                        result.region_physical.y,
                        result.region_physical.width,
                        result.region_physical.height,
                        (result.dpi / 96.0) * 100.0
                    );

                    // Update backend state with new region
                    let state = app.state::<AppState>();
                    let new_region = apply_selector_region(&state, &result);

                    // Save to config
                    {
                        let mut config = lock_or_recover(&state.config);
                        config.last_capture_region = Some(new_region);
                        crate::config_save::save_or_warn(app, &config, "the capture region");
                    }
                }
            }
        }

        "Selector.Cancelled" => {
            info!("❌ Area selection cancelled");
        }

        "Region.Updated" => {
            info!("📍 Region updated by user (drag/resize)");
            // TODO: Update backend state if we want live updates
        }

        "Overlay.SettingsClicked" | "Overlay.SettingsRequested" => {
            info!("⚙️ Settings button clicked - bringing main window to front");
            // TODO: Focus main settings window
        }

        _ => {
            warn!("⚠️ Unknown IPC message type: {}", message.message_type);
        }
    }
}

/// Record the region the selector just confirmed, replacing the old one.
///
/// The persisted copy (`config.last_capture_region`) and the disk write stay
/// with the caller: this function only commits the in-memory region, so it can
/// be exercised without a running Tauri app.
fn apply_selector_region(state: &AppState, result: &SelectorResultPayload) -> CaptureRegion {
    let new_region = CaptureRegion {
        x: result.region_physical.x,
        y: result.region_physical.y,
        width: result.region_physical.width,
        height: result.region_physical.height,
    };

    *lock_or_recover(&state.capture_region) = Some(new_region);
    new_region
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::protocol::RegionData;

    #[test]
    fn the_wire_payload_is_what_overlay_host_sends() {
        let payload = serde_json::json!({
            "regionPhysical": {
                "x": 10,
                "y": 20,
                "width": 640,
                "height": 120,
                "coordSpace": "physical",
            },
            "sourceMonitor": "DISPLAY1",
            "dpi": 144.0,
        });

        let result: SelectorResultPayload =
            serde_json::from_value(payload).expect("parse selector result");

        assert_eq!(result.region_physical.x, 10);
        assert_eq!(result.region_physical.y, 20);
        assert_eq!(result.region_physical.width, 640);
        assert_eq!(result.region_physical.height, 120);
        assert_eq!(result.region_physical.coord_space, "physical");
        assert_eq!(result.source_monitor.as_deref(), Some("DISPLAY1"));
        assert_eq!(result.dpi, 144.0);
    }

    #[test]
    fn a_selector_result_replaces_the_capture_region() {
        let state = AppState::default();
        let result = SelectorResultPayload {
            region_physical: RegionData {
                x: 10,
                y: 20,
                width: 640,
                height: 120,
                coord_space: "physical".to_string(),
                monitor_id: None,
            },
            source_monitor: None,
            dpi: 144.0,
        };

        let region = apply_selector_region(&state, &result);

        assert_eq!(region.x, 10);
        assert_eq!(region.y, 20);
        assert_eq!(region.width, 640);
        assert_eq!(region.height, 120);
        let committed = lock_or_recover(&state.capture_region);
        assert_eq!(committed.as_ref(), Some(&region));
    }
}
