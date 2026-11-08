//! Correlates each selection request with the display selector's native token.

use serde::Serialize;
use std::future::Future;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};
use tokio::sync::oneshot;

use crate::app_state::AppState;
use crate::config::CaptureRegion;
use crate::sync_utils::lock_or_recover;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionRequest {
    pub request_id: String,
    pub start_translation: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionOutcome {
    pub request_id: String,
    pub region: Option<CaptureRegion>,
}

struct PendingSelection {
    request: SelectionRequest,
    selection_id: u64,
    sender: oneshot::Sender<SelectionOutcome>,
}

#[derive(Default)]
pub struct SelectionRequests {
    control: tokio::sync::Mutex<()>,
    pending: Mutex<Option<PendingSelection>>,
}

impl SelectionRequests {
    fn begin(
        &self,
        request: SelectionRequest,
        select: impl FnOnce() -> u64,
    ) -> Result<oneshot::Receiver<SelectionOutcome>, String> {
        if request.request_id.is_empty() || request.request_id.len() > 128 {
            return Err("Invalid selection request".into());
        }
        let mut pending = lock_or_recover(&self.pending);
        if pending.is_some() {
            return Err("An area selection is already open".into());
        }
        let selection_id = select();
        let (sender, receiver) = oneshot::channel();
        *pending = Some(PendingSelection {
            request,
            selection_id,
            sender,
        });
        Ok(receiver)
    }

    pub fn current(&self) -> Option<SelectionRequest> {
        lock_or_recover(&self.pending)
            .as_ref()
            .map(|pending| pending.request.clone())
    }

    fn selection_id(&self, request_id: &str) -> Result<u64, String> {
        lock_or_recover(&self.pending)
            .as_ref()
            .filter(|pending| pending.request.request_id == request_id)
            .map(|pending| pending.selection_id)
            .ok_or_else(|| "The area selection is no longer active".into())
    }

    fn finish(&self, request_id: &str, region: Option<CaptureRegion>) -> Result<(), String> {
        self.selection_id(request_id)?;
        if let Some(pending) = lock_or_recover(&self.pending).take() {
            let _ = pending.sender.send(SelectionOutcome {
                request_id: request_id.into(),
                region,
            });
        }
        Ok(())
    }

    async fn complete<F, Fut>(
        &self,
        request_id: &str,
        region: Option<CaptureRegion>,
        selection_id: Option<u64>,
        apply: F,
    ) -> Result<(), String>
    where
        F: FnOnce(u64) -> Fut,
        Fut: Future<Output = Result<(), String>>,
    {
        let _control = self.control.lock().await;
        let token = self.selection_id(request_id)?;
        if region.is_some() && selection_id != Some(token) {
            return Err(crate::capture_target::TargetError::ExpiredSelection.to_string());
        }
        // Native display validation, hide and persistence may await. Only this
        // async lock spans them; metadata remains readable by the selector.
        apply(token).await?;
        self.finish(request_id, region)
    }
}

#[tauri::command]
pub async fn request_area_selection(
    app: AppHandle,
    state: State<'_, AppState>,
    request_id: String,
    start_translation: bool,
) -> Result<SelectionOutcome, String> {
    state.startup_gate.wait_until_ready().await?;
    let receiver = {
        let _control = state.area_selection.control.lock().await;
        let _transition = state.session_lifecycle.control.lock().await;
        if state.is_translation_running() {
            return Err("Stop translation before selecting another area.".into());
        }
        let monitor = crate::display_monitor::current(&app)?;
        let request = SelectionRequest {
            request_id,
            start_translation,
        };
        let receiver = state.area_selection.begin(request.clone(), || {
            lock_or_recover(&state.capture).begin_selection(monitor.clone())
        })?;
        let selection_id = state.area_selection.selection_id(&request.request_id)?;
        // The WinUI experiment does not return a correlated selection token.
        if let Err(error) = crate::selector_window::open_legacy(
            app.clone(),
            &state.selector_snapshot,
            &monitor,
            selection_id,
        )
        .await
        {
            lock_or_recover(&state.capture).cancel_selection();
            let _ = crate::selector_window::close(&app, &state.selector_snapshot);
            let _ = state.area_selection.finish(&request.request_id, None);
            return Err(error);
        }
        receiver
    };
    receiver
        .await
        .map_err(|_| "Area selection interrupted".into())
}

#[tauri::command]
pub fn get_area_selection_request(state: State<'_, AppState>) -> Option<SelectionRequest> {
    state.area_selection.current()
}

#[tauri::command]
pub async fn complete_area_selection(
    app: AppHandle,
    state: State<'_, AppState>,
    request_id: String,
    region: Option<CaptureRegion>,
    scale_factor: Option<f64>,
    selection_id: Option<u64>,
) -> Result<(), String> {
    let state = &*state;
    state
        .area_selection
        .complete(&request_id, region, selection_id, |token| async move {
            if let Some(region) = region {
                let scale = scale_factor.ok_or("Confirmation requires a DPI scale factor")?;
                crate::app_state::validate_capture_region(region.width, region.height, scale)?;
                let monitors = crate::display_monitor::available(&app)?;
                lock_or_recover(&state.capture)
                    .selection_to_confirm(token, region, &monitors)
                    .map_err(|error| error.to_string())?;
                let snapshot = crate::selector_window::snapshot(&state.selector_snapshot);
                crate::selector_window::close(&app, &state.selector_snapshot)?;
                if let Err(error) = crate::display_session::set_region(
                    &app,
                    state,
                    region,
                    scale,
                    Some(token),
                    "selector",
                )
                .await
                {
                    *lock_or_recover(&state.selector_snapshot) = snapshot;
                    if let Some(window) = app.get_webview_window("selector") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                    return Err(error);
                }
            } else {
                crate::selector_window::close(&app, &state.selector_snapshot)?;
            }
            lock_or_recover(&state.capture).cancel_selection();
            Ok(())
        })
        .await
}

pub async fn cancel_from_window(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let _control = state.area_selection.control.lock().await;
    if let Some(request) = state.area_selection.current() {
        crate::selector_window::close(app, &state.selector_snapshot)?;
        lock_or_recover(&state.capture).cancel_selection();
        state.area_selection.finish(&request.request_id, None)?;
    }
    Ok(())
}

pub async fn cancel_after_destroy(app: &AppHandle) {
    let state = app.state::<AppState>();
    let _control = state.area_selection.control.lock().await;
    if let Some(request) = state.area_selection.current() {
        *lock_or_recover(&state.selector_snapshot) = None;
        lock_or_recover(&state.capture).cancel_selection();
        let _ = state.area_selection.finish(&request.request_id, None);
    }
}

#[cfg(test)]
#[path = "area_selection_tests.rs"]
mod tests;
