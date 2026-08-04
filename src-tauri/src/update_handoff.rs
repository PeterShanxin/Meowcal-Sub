// =============================================================================
// UPDATE HANDOFF - what has to stop before an installer replaces our files
// =============================================================================
// Applying an update runs the NSIS installer over the directory this app is
// executing from, and the updater plugin exits the process to let it. That exit
// is not `RunEvent::Exit`: none of the cleanup wired into the normal quit path
// runs, so the work has to happen before the handoff instead of after it.
//
// Three things are at stake, in order of how badly they fail:
//
//   1. `OverlayHost.exe` lives inside the install directory. A running copy
//      holds its own image file open, and NSIS cannot replace a file that is
//      open. `process_lifetime`'s job object does end it when we exit, but that
//      is a race against an installer already retrying; stopping it first is
//      not a race.
//   2. The translation engine holds a multi-gigabyte model resident. It exits
//      with us either way, but ending it here means the installer is not
//      competing with a process still writing its own shutdown.
//   3. Window geometry is normally persisted from `RunEvent::Exit`. Skipping it
//      reopens the updated app in the wrong place, which reads as the update
//      having lost the user's settings.
//
// Deliberately not a no-op when translation is stopped: the overlay window and
// the engine can both be alive without a session running.
// =============================================================================

use crate::commands::{stop_translation, AppState};
use crate::sync_utils::lock_or_recover;
use tauri::{AppHandle, State};
use tracing::info;

/// Bring the app to a state an installer can overwrite.
///
/// Called from the front end immediately before `downloadAndInstall`.
///
/// Stopping a session is best effort, and deliberately so: `stop_translation`
/// signals the pipeline loop and returns without waiting for it, and every step
/// inside it that can fail is already tolerated rather than reported. The loop
/// holds nothing inside the install directory, so what it is still doing when
/// the installer starts does not decide whether the upgrade succeeds. Do not
/// read the `Result` here as a promise that capture has stopped.
///
/// The two child processes are different: they hold files open, so this waits
/// for them.
#[tauri::command]
pub async fn prepare_for_update(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    info!("Preparing for an in-place update…");

    // Scoped so the guard is dropped before the await: a `MutexGuard` held
    // across one makes this future non-Send and the command will not compile.
    let running = { *lock_or_recover(&state.is_running) };
    if running {
        stop_translation(state, app.clone()).await?;
    }

    crate::window_lifecycle::persist_main_geometry_from_app(&app);

    // `kill` then `wait` on a child that is slow to die would otherwise park a
    // tokio worker while the front end is awaiting this command.
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::overlay_host_process::stop(&handle);
        crate::hy_mt_runtime::shutdown_owned();
    })
    .await
    .map_err(|error| format!("Could not stop this app's child processes: {error}"))?;

    info!("Ready for the installer to replace this installation");
    Ok(())
}
