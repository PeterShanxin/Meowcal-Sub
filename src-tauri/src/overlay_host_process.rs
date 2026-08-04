// =============================================================================
// OVERLAY HOST PROCESS - ownership of the WinUI child
// =============================================================================
// The WinUI OverlayHost is an executable shipped inside the install directory,
// so anything that replaces that directory - an upgrade installer, most of all -
// needs it stopped first. Ownership lives here rather than in `main` so both the
// exit path and the update handoff can reach it.
//
// `process_lifetime`'s job object still terminates it however this process ends;
// this is the graceful path, not the guarantee.
// =============================================================================

use crate::sync_utils::lock_or_recover;
use std::process::Child;
use std::sync::{Arc, Mutex};
use tauri::Manager;
use tracing::info;

/// Managed state for tracking the OverlayHost child process.
pub struct OverlayHostProcess(Arc<Mutex<Option<Child>>>);

impl OverlayHostProcess {
    /// `None` records that no host is running, which is the default: the WinUI
    /// host is opt-in behind an environment variable.
    pub fn new(child: Option<Child>) -> Self {
        Self(Arc::new(Mutex::new(child)))
    }
}

/// Stop the OverlayHost child if this app started one.
///
/// Taking the child out of the state makes the call idempotent: the exit path
/// and the update handoff can both run without the second one waiting on a
/// process that is already gone.
pub fn stop<M: Manager<tauri::Wry>>(manager: &M) {
    let Some(process) = manager.try_state::<OverlayHostProcess>() else {
        return;
    };
    let Some(mut child) = lock_or_recover(&process.0).take() else {
        return;
    };
    let _ = child.kill();
    let _ = child.wait();
    info!("🛑 Stopped OverlayHost process");
}
