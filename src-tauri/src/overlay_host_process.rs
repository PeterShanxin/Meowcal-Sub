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
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use tauri::Manager;
use tracing::{info, warn};

/// Managed state for tracking the OverlayHost child process.
pub struct OverlayHostProcess(Arc<Mutex<Option<Child>>>);

impl OverlayHostProcess {
    /// `None` records that no host is running, which is the default: the WinUI
    /// host is opt-in behind an environment variable.
    pub fn new(child: Option<Child>) -> Self {
        Self(Arc::new(Mutex::new(child)))
    }
}

/// Where the host executable can be, most specific first.
///
/// The installed layout puts it beside the app in the resource directory; the
/// rest are the shapes a development run takes, which differ by whether the
/// process was started from the repository root or from `src-tauri`.
fn candidates<M: Manager<tauri::Wry>>(manager: &M) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(resource_dir) = manager.path().resource_dir() {
        candidates.push(resource_dir.join("OverlayHost.exe"));
    }

    if let Ok(current_dir) = std::env::current_dir() {
        if current_dir
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("src-tauri"))
        {
            candidates.push(current_dir.join("resources").join("OverlayHost.exe"));
        }
        candidates.push(
            current_dir
                .join("src-tauri")
                .join("resources")
                .join("OverlayHost.exe"),
        );
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("OverlayHost.exe"));
        }
    }

    candidates
}

/// Start the host and hand it to the app to own.
///
/// Always leaves an `OverlayHostProcess` managed, even when nothing started, so
/// that the stop path has something to find and does not have to distinguish
/// "no host" from "state never registered".
pub fn spawn_and_manage<M: Manager<tauri::Wry>>(manager: &M) {
    let candidates = candidates(manager);
    let Some(path) = candidates.iter().find(|candidate| candidate.exists()) else {
        warn!("⚠️ OverlayHost.exe not found. Tried: {:?}", candidates);
        manager.manage(OverlayHostProcess::new(None));
        return;
    };

    info!("🚀 Spawning OverlayHost from: {:?}", path);
    match Command::new(path).spawn() {
        Ok(child) => {
            info!("✅ OverlayHost spawned (PID: {})", child.id());
            // Tied to this process, so a crash cannot leave it holding its own
            // image file open inside the install directory.
            crate::process_lifetime::attach_to_app_lifetime(&child);
            manager.manage(OverlayHostProcess::new(Some(child)));
        }
        Err(error) => {
            warn!("⚠️ Failed to spawn OverlayHost: {}", error);
            manager.manage(OverlayHostProcess::new(None));
        }
    }
}

/// Stop the OverlayHost child if this app started one.
///
/// Blocking: it waits for the child to actually exit, because the caller's next
/// move is usually to let something else have the file the child was running
/// from.
pub fn stop<M: Manager<tauri::Wry>>(manager: &M) {
    let Some(process) = manager.try_state::<OverlayHostProcess>() else {
        return;
    };
    if stop_child(&process.0) {
        info!("🛑 Stopped OverlayHost process");
    }
}

/// Take the child out of the slot and end it. Returns whether there was one.
///
/// Taking rather than borrowing is what makes the call idempotent: the exit
/// path and the update handoff can both run without the second one waiting on a
/// process that is already gone.
fn stop_child(slot: &Mutex<Option<Child>>) -> bool {
    let Some(mut child) = lock_or_recover(slot).take() else {
        return false;
    };
    let _ = child.kill();
    let _ = child.wait();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both the exit path and the update handoff call this, and on the update
    /// path they can both run within a second of each other. The second call
    /// must not block on a process that no longer exists.
    #[test]
    fn stopping_a_second_time_finds_nothing_left_to_stop() {
        let child = std::process::Command::new("ping")
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("the fixture process should start");
        let slot = Mutex::new(Some(child));

        assert!(stop_child(&slot), "the first call owns the child");
        assert!(!stop_child(&slot), "the second call has nothing to own");
    }

    #[test]
    fn stopping_without_a_host_is_not_an_error() {
        assert!(!stop_child(&Mutex::new(None)));
    }
}
