use crate::commands::AppState;
use crate::config::{save_config, WindowPreferences};
use crate::sync_utils::lock_or_recover;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{
    webview::{PageLoadEvent, PageLoadPayload},
    AppHandle, CloseRequestApi, Emitter, Manager, Monitor, PhysicalPosition, PhysicalSize, Runtime,
    Webview, Window, WindowEvent,
};

const MIN_WINDOW_WIDTH: u32 = 320;
const MIN_WINDOW_HEIGHT: u32 = 300;
static MAIN_GEOMETRY_READY: AtomicBool = AtomicBool::new(false);
static MAIN_PAGE_READY: AtomicBool = AtomicBool::new(false);
static MAIN_SHOW_SCHEDULED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq)]
struct Geometry {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale_factor: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseBehavior {
    HideMainToTray,
    HideWizard,
    AllowClose,
}

pub fn close_behavior(window_label: &str) -> CloseBehavior {
    match window_label {
        "main" => CloseBehavior::HideMainToTray,
        "foundry-wizard" => CloseBehavior::HideWizard,
        _ => CloseBehavior::AllowClose,
    }
}

pub fn handle_close_requested<R: Runtime>(window: &Window<R>, api: &CloseRequestApi) {
    match close_behavior(window.label()) {
        CloseBehavior::HideMainToTray => {
            api.prevent_close();
            let _ = window.hide();
        }
        CloseBehavior::HideWizard => {
            api.prevent_close();
            let _ = window.emit("wizard-window-hidden", ());
            let _ = window.app_handle().emit(
                "foundry-wizard-closed",
                serde_json::json!({
                    "modelDownloaded": false,
                    "selectedModel": null,
                    "closedViaX": true
                }),
            );
            let _ = window.hide();
        }
        CloseBehavior::AllowClose => {}
    }
}

pub fn remember_main_geometry(window: &Window) {
    if window.label() != "main" {
        return;
    }
    let Some(state) = window.try_state::<AppState>() else {
        return;
    };
    capture_preferences(
        window,
        &mut lock_or_recover(&state.config).window_preferences,
    );
}

pub fn persist_main_geometry(window: &Window) {
    remember_main_geometry(window);
    let Some(state) = window.try_state::<AppState>() else {
        return;
    };
    let config = lock_or_recover(&state.config).clone();
    let _ = save_config(window.app_handle(), &config);
}

pub fn persist_main_geometry_from_app(app: &AppHandle) {
    if let Some(webview_window) = app.get_webview_window("main") {
        persist_main_geometry(&webview_window.as_ref().window());
    }
}

pub fn handle_geometry_event(window: &Window, event: &WindowEvent) {
    if window.label() != "main" {
        return;
    }
    match event {
        WindowEvent::Moved(_)
        | WindowEvent::Resized(_)
        | WindowEvent::ScaleFactorChanged { .. } => {
            remember_main_geometry(window);
        }
        WindowEvent::CloseRequested { .. } => persist_main_geometry(window),
        _ => {}
    }
}

pub fn handle_page_load(webview: &Webview, payload: &PageLoadPayload<'_>) {
    if webview.label() == "main" && payload.event() == PageLoadEvent::Finished {
        MAIN_PAGE_READY.store(true, Ordering::Release);
        show_main_if_ready(webview.app_handle());
    }
}

fn capture_preferences(window: &Window, preferences: &mut WindowPreferences) {
    let is_maximized = window.is_maximized().unwrap_or(preferences.is_maximized);
    preferences.is_maximized = is_maximized;
    preferences.scale_factor = window.scale_factor().ok().or(preferences.scale_factor);
    if is_maximized {
        return;
    }
    if let Ok(size) = window.inner_size() {
        preferences.width = Some(size.width);
        preferences.height = Some(size.height);
    }
    if let Ok(position) = window.outer_position() {
        preferences.x = Some(position.x);
        preferences.y = Some(position.y);
    }
}

/// Restore saved geometry before first visibility to avoid a center-then-jump.
pub fn restore_and_show_main(app: &AppHandle, preferences: &WindowPreferences) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.hide();
    if let (Some(x), Some(y), Some(width), Some(height)) = (
        preferences.x,
        preferences.y,
        preferences.width,
        preferences.height,
    ) {
        let saved = Geometry {
            x,
            y,
            width,
            height,
            scale_factor: preferences.scale_factor.unwrap_or(0.0),
        };
        let monitors = window
            .available_monitors()
            .unwrap_or_default()
            .iter()
            .map(monitor_geometry)
            .collect::<Vec<_>>();
        let primary = window
            .primary_monitor()
            .ok()
            .flatten()
            .as_ref()
            .map(monitor_geometry);
        let fitted = fit_geometry(saved, &monitors, primary);
        let _ = window.set_size(PhysicalSize::new(fitted.width, fitted.height));
        let _ = window.set_position(PhysicalPosition::new(fitted.x, fitted.y));
    } else {
        if let (Some(width), Some(height)) = (preferences.width, preferences.height) {
            let _ = window.set_size(PhysicalSize::new(width, height));
        }
        let _ = window.center();
    }
    if preferences.is_maximized {
        let _ = window.maximize();
    }
    MAIN_GEOMETRY_READY.store(true, Ordering::Release);
    show_main_if_ready(app);
}

fn show_main_if_ready(app: &AppHandle) {
    if MAIN_GEOMETRY_READY.load(Ordering::Acquire) && MAIN_PAGE_READY.load(Ordering::Acquire) {
        if let Some(window) = app.get_webview_window("main") {
            if MAIN_SHOW_SCHEDULED.swap(true, Ordering::AcqRel) {
                return;
            }
            tauri::async_runtime::spawn(async move {
                let mut previous = None;
                for _ in 0..80 {
                    let current = window
                        .inner_size()
                        .ok()
                        .filter(|size| {
                            size.width >= MIN_WINDOW_WIDTH && size.height >= MIN_WINDOW_HEIGHT
                        })
                        .zip(window.outer_position().ok());
                    if current.is_some() && current == previous {
                        let _ = window.show();
                        return;
                    }
                    previous = current;
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                let _ = window.show();
            });
        }
    }
}

fn monitor_geometry(monitor: &Monitor) -> Geometry {
    let work_area = monitor.work_area();
    Geometry {
        x: work_area.position.x,
        y: work_area.position.y,
        width: work_area.size.width,
        height: work_area.size.height,
        scale_factor: monitor.scale_factor(),
    }
}

fn fit_geometry(saved: Geometry, monitors: &[Geometry], primary: Option<Geometry>) -> Geometry {
    let target = monitors
        .iter()
        .copied()
        .max_by_key(|monitor| intersection_area(saved, *monitor))
        .filter(|monitor| intersection_area(saved, *monitor) > 0)
        .or(primary)
        .or_else(|| monitors.first().copied());
    let Some(target) = target else {
        return saved;
    };
    let target_scale = valid_scale(target.scale_factor);
    let saved_scale = if saved.scale_factor.is_finite() && saved.scale_factor > 0.0 {
        saved.scale_factor
    } else {
        target_scale
    };
    let scale_ratio = target_scale / saved_scale;
    let width = scaled_dimension(saved.width, scale_ratio, target.width, MIN_WINDOW_WIDTH);
    let height = scaled_dimension(saved.height, scale_ratio, target.height, MIN_WINDOW_HEIGHT);
    let max_x = target
        .x
        .saturating_add((target.width.saturating_sub(width)) as i32);
    let max_y = target
        .y
        .saturating_add((target.height.saturating_sub(height)) as i32);
    Geometry {
        x: saved.x.clamp(target.x, max_x),
        y: saved.y.clamp(target.y, max_y),
        width,
        height,
        scale_factor: target.scale_factor,
    }
}

fn valid_scale(scale: f64) -> f64 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

fn scaled_dimension(value: u32, ratio: f64, available: u32, minimum: u32) -> u32 {
    let scaled = (value as f64 * ratio).round().clamp(1.0, u32::MAX as f64) as u32;
    scaled.clamp(minimum.min(available), available)
}

fn intersection_area(left: Geometry, right: Geometry) -> i64 {
    let left_right = i64::from(left.x) + i64::from(left.width);
    let right_right = i64::from(right.x) + i64::from(right.width);
    let left_bottom = i64::from(left.y) + i64::from(left.height);
    let right_bottom = i64::from(right.y) + i64::from(right.height);
    let width = (left_right.min(right_right) - i64::from(left.x.max(right.x))).max(0);
    let height = (left_bottom.min(right_bottom) - i64::from(left.y.max(right.y))).max(0);
    width * height
}

#[cfg(test)]
mod tests {
    use super::{close_behavior, fit_geometry, CloseBehavior, Geometry};

    #[test]
    fn main_window_hides_to_tray_instead_of_destroying() {
        assert_eq!(close_behavior("main"), CloseBehavior::HideMainToTray);
    }

    #[test]
    fn setup_wizard_remains_reopenable() {
        assert_eq!(close_behavior("foundry-wizard"), CloseBehavior::HideWizard);
    }

    #[test]
    fn transient_windows_can_close_normally() {
        assert_eq!(close_behavior("selector"), CloseBehavior::AllowClose);
    }

    #[test]
    fn missing_monitor_falls_back_inside_primary_work_area() {
        let saved = geometry(2500, 100, 480, 600, 1.0);
        let primary = geometry(0, 0, 1920, 1040, 1.0);

        let fitted = fit_geometry(saved, &[primary], Some(primary));

        assert_eq!(fitted, geometry(1440, 100, 480, 600, 1.0));
    }

    #[test]
    fn restore_preserves_logical_size_across_dpi_change() {
        let saved = geometry(100, 100, 600, 750, 1.25);
        let monitor = geometry(0, 0, 1920, 1040, 1.0);

        let fitted = fit_geometry(saved, &[monitor], Some(monitor));

        assert_eq!(fitted, geometry(100, 100, 480, 600, 1.0));
    }

    #[test]
    fn legacy_geometry_without_dpi_preserves_physical_size() {
        let saved = geometry(100, 100, 600, 750, 0.0);
        let monitor = geometry(0, 0, 2400, 1300, 1.25);

        let fitted = fit_geometry(saved, &[monitor], Some(monitor));

        assert_eq!(fitted, geometry(100, 100, 600, 750, 1.25));
    }

    #[test]
    fn restore_selects_monitor_with_largest_saved_overlap() {
        let left = geometry(-1920, 0, 1920, 1040, 1.0);
        let primary = geometry(0, 0, 1920, 1040, 1.0);
        let saved = geometry(-400, 100, 700, 600, 1.0);

        let fitted = fit_geometry(saved, &[left, primary], Some(primary));

        assert_eq!(fitted.x, -700);
        assert_eq!(fitted.y, 100);
    }

    fn geometry(x: i32, y: i32, width: u32, height: u32, scale_factor: f64) -> Geometry {
        Geometry {
            x,
            y,
            width,
            height,
            scale_factor,
        }
    }
}
