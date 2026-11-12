use std::sync::atomic::{AtomicBool, Ordering};
use tauri::AppHandle;

static MAIN_STARTUP_ALLOWED: AtomicBool = AtomicBool::new(false);

pub fn main_startup_allowed() -> bool {
    MAIN_STARTUP_ALLOWED.load(Ordering::Acquire)
}

pub(crate) fn ready_to_show(geometry: &AtomicBool, page: &AtomicBool) -> bool {
    main_startup_allowed() && geometry.load(Ordering::Acquire) && page.load(Ordering::Acquire)
}

#[tauri::command]
pub fn show_main_after_setup(app: AppHandle) {
    MAIN_STARTUP_ALLOWED.store(true, Ordering::Release);
    crate::window_lifecycle::show_main_if_ready(&app);
}

#[tauri::command]
pub fn quit_initial_setup(app: AppHandle) {
    app.exit(0);
}
