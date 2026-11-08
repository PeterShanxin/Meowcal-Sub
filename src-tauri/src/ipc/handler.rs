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
                    if state.is_translation_running() {
                        return;
                    }
                    let monitors = match crate::display_monitor::available(app) {
                        Ok(monitors) if monitors.len() == 1 => monitors,
                        _ => return,
                    };
                    let selection = match apply_selector_region(&state, &result, &monitors) {
                        Ok(selection) => selection,
                        Err(error) => {
                            warn!(%error, "Rejected native selector result");
                            return;
                        }
                    };

                    // Save to config
                    {
                        let mut config = lock_or_recover(&state.config);
                        config.last_capture_region = Some(selection.region);
                        config.last_capture_scale_factor = Some(selection.monitor.scale_factor);
                        config.last_capture_monitor = Some(selection.monitor);
                        crate::config_save::save_or_warn(app, &config, "the capture region");
                    }
                }
            }
        }

        "Selector.Cancelled" => {
            lock_or_recover(&app.state::<AppState>().capture).cancel_selection();
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
fn apply_selector_region(
    state: &AppState,
    result: &SelectorResultPayload,
    monitors: &[crate::capture_target::CaptureMonitor],
) -> Result<crate::capture_target::CaptureSelection, crate::capture_target::TargetError> {
    let physical = CaptureRegion::new(
        result.region_physical.x,
        result.region_physical.y,
        result.region_physical.width,
        result.region_physical.height,
    );
    let mut capture = lock_or_recover(&state.capture);
    let selection = capture.confirm_native(physical, monitors)?;
    capture.cancel_selection();
    state.pipeline_clock.invalidate_capture();
    Ok(selection)
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

        let monitor = crate::capture_target::CaptureMonitor {
            device_id: "screen".into(),
            bounds: CaptureRegion::new(0, 0, 1920, 1080),
            scale_factor: 1.0,
        };
        lock_or_recover(&state.capture).begin_selection(monitor.clone());
        let selection = apply_selector_region(&state, &result, &[monitor]).unwrap();
        let region = selection.region;

        assert_eq!(region.x, 10);
        assert_eq!(region.y, 20);
        assert_eq!(region.width, 640);
        assert_eq!(region.height, 120);
        assert_eq!(state.current_capture_region(), Some(region));
    }
}
