//! Bind selection and translation to one display. Moving Home during a running
//! session does not move capture; losing the bound surface ends the session.

use crate::app_state::AppState;
use crate::capture_target::{CaptureSelection, TargetError};
use crate::config::CaptureRegion;
use crate::display_monitor;
use crate::sync_utils::lock_or_recover;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::watch;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayContext {
    pub multiple_displays: bool,
    pub region: Option<CaptureRegion>,
}

/// Tauri does not expose WM_DISPLAYCHANGE. Detect idle hotplug as well as
/// changes during translation, without adding a frontend polling loop.
pub fn watch_topology(app: AppHandle, state: &AppState) {
    let task = tauri::async_runtime::spawn(async move {
        let mut previous = None;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let checking_app = app.clone();
            let current = tauri::async_runtime::spawn_blocking(move || {
                display_monitor::available(&checking_app)
            })
            .await
            .ok()
            .and_then(Result::ok);
            if current != previous {
                let _ = app.emit("display-context-changed", ());
                previous = current;
            }
        }
    });
    *lock_or_recover(&state.topology_watch) = Some(task);
}

pub fn stop_topology_watch(state: &AppState) {
    if let Some(task) = lock_or_recover(&state.topology_watch).take() {
        task.abort();
    }
}

pub fn context(app: &AppHandle, state: &AppState) -> Result<DisplayContext, String> {
    let monitors = display_monitor::available(app)?;
    let current = display_monitor::current(app)?;
    let running = state.is_translation_running();
    let region = lock_or_recover(&state.capture).visible_region(&current, running, &monitors);
    Ok(DisplayContext {
        multiple_displays: monitors.len() > 1,
        region,
    })
}

pub fn for_start(app: &AppHandle, state: &AppState) -> Result<CaptureSelection, String> {
    let selection = state
        .current_capture_selection()
        .ok_or_else(|| TargetError::SelectOnCurrentDisplay.to_string())?;
    selection
        .validate_start(
            &display_monitor::current(app)?,
            &display_monitor::available(app)?,
        )
        .map_err(|e| e.to_string())?;
    Ok(selection)
}

pub fn invalidate(app: &AppHandle, state: &AppState, session_id: u64) -> bool {
    if !state.lose_capture_display(session_id) {
        return false;
    }
    tracing::warn!(session_id, "Capture display changed; stopping translation");
    let _ = crate::overlay::hide_overlay(app);
    let _ = app.emit(
        "display-invalidated",
        TargetError::DisplayChanged.to_string(),
    );
    true
}

pub async fn set_region(
    app: &AppHandle,
    state: &AppState,
    region: CaptureRegion,
    scale: f64,
    selection_id: Option<u64>,
    window_label: &str,
) -> Result<(), String> {
    let _transition = state.session_lifecycle.control.lock().await;
    crate::app_state::validate_capture_region(region.width, region.height, scale)?;
    let monitors = display_monitor::available(app)?;
    let main_monitor = (window_label == "main" && !state.is_translation_running())
        .then(|| display_monitor::current(app))
        .transpose()?;
    let selection = {
        let capture = lock_or_recover(&state.capture);
        if window_label == "selector" && !state.is_translation_running() {
            capture
                .selection_to_confirm(
                    selection_id.ok_or_else(|| TargetError::ExpiredSelection.to_string())?,
                    region,
                    &monitors,
                )
                .map_err(|e| e.to_string())?
        } else if let Some(monitor) = main_monitor {
            if (monitor.scale_factor - scale).abs() > 0.001 {
                return Err(TargetError::DisplayChanged.to_string());
            }
            CaptureSelection::new(region, monitor).map_err(|e| e.to_string())?
        } else if matches!(window_label, "overlay" | "main") && state.is_translation_running() {
            let selected = capture
                .selected
                .as_ref()
                .ok_or_else(|| TargetError::DisplayChanged.to_string())?;
            if !selected.monitor.is_present(&monitors)
                || (selected.monitor.scale_factor - scale).abs() > 0.001
            {
                return Err(TargetError::DisplayChanged.to_string());
            }
            CaptureSelection::new(region, selected.monitor.clone()).map_err(|e| e.to_string())?
        } else {
            return Err(TargetError::ExpiredSelection.to_string());
        }
    };
    persist(app, &selection).await
}

async fn persist(app: &AppHandle, selection: &CaptureSelection) -> Result<(), String> {
    let app = app.clone();
    let selection = selection.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        crate::config_save::update_live(
            &state.config,
            |config| {
                config.last_capture_region = Some(selection.region);
                config.last_capture_scale_factor = Some(selection.monitor.scale_factor);
                config.last_capture_monitor = Some(selection.monitor.clone());
            },
            |config| {
                crate::config::save_config(&app, config)?;
                state.set_capture_selection(selection.clone());
                Ok(())
            },
        )
        .map(|_| ())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Independent from OCR/inference awaits, so a lost display cannot leave a
/// translation running against stale pixels until the model returns.
pub fn watch(
    app: AppHandle,
    selection: CaptureSelection,
    session_id: u64,
    mut stopped: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {},
                _ = stopped.changed() => break,
            }
            let state = app.state::<AppState>();
            if !state.pipeline_clock.is_session_current(session_id)
                || !state.is_translation_running()
            {
                break;
            }
            let checking_app = app.clone();
            let monitor = selection.monitor.clone();
            let valid = tauri::async_runtime::spawn_blocking(move || {
                display_monitor::require_present(&checking_app, &monitor)?;
                display_monitor::require_overlay(&checking_app, &monitor)
            })
            .await
            .is_ok_and(|result| result.is_ok());
            if valid {
                continue;
            }
            let state = app.state::<AppState>();
            let _transition = state.session_lifecycle.control.lock().await;
            if !invalidate(&app, &state, session_id) {
                break;
            }
            crate::overlay_ipc::send_overlay_message(
                &app,
                crate::ipc::IpcMessage::new("Overlay.Hide"),
            )
            .await;
            if let Err(error) = state.session_lifecycle.wait_stopped().await {
                tracing::warn!(%error, "Display change capture cleanup failed");
            }
            break;
        }
    })
}
