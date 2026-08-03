// =============================================================================
// PROCESS LIFETIME - children die when this process dies
// =============================================================================
// The translation engine is a separate executable (`llama-server.exe`) holding
// a multi-gigabyte model resident. `hy_mt_runtime::shutdown_owned` kills it on
// the way out, but that only runs on the graceful path - `RunEvent::Exit`.
//
// Windows does not tie a child's lifetime to its parent's. Every other way the
// app can end leaves the engine running with the model still in memory:
//
//   - the installer closing the running app to replace its files,
//   - Task Manager, `Stop-Process`, or any other TerminateProcess,
//   - a panic that aborts, or the WebView taking the process down with it,
//   - session logoff and shutdown, where Exit is not delivered.
//
// Each of those leaks one engine, and the next launch cannot reuse it because
// its port is taken, so it starts another. They accumulate one per crash until
// the machine is out of memory.
//
// The fix is a job object created with JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE. Every
// child is assigned to it, and the only handle to it is held by this process.
// However this process ends, Windows closes that handle, and closing the last
// handle to the job terminates everything inside it. No cleanup code has to run,
// which is the point: the paths that leak are exactly the paths where our code
// does not get to run.
// =============================================================================

// Which processes qualify is decided in `process_ownership`, away from the
// Win32 calls that gather the evidence and act on it.

#[cfg(target_os = "windows")]
mod windows_impl {
    use crate::process_ownership::{normalize, orphans, ProcessEntry};
    use std::path::{Path, PathBuf};
    use std::process::Child;
    use std::sync::OnceLock;
    use tracing::{debug, info, warn};
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, TerminateProcess, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    };

    /// The job every child joins. Stored as an integer because a raw `HANDLE`
    /// is not `Send`; the value is only ever handed back to Win32.
    ///
    /// Never closed on purpose. The handle has to outlive every child, and the
    /// process dying is what closes it - that is the mechanism, not a leak.
    static JOB: OnceLock<Option<isize>> = OnceLock::new();

    fn job_handle() -> Option<HANDLE> {
        JOB.get_or_init(|| unsafe { create_job() })
            .map(|raw| HANDLE(raw as *mut std::ffi::c_void))
    }

    unsafe fn create_job() -> Option<isize> {
        let job = match CreateJobObjectW(None, windows::core::PCWSTR::null()) {
            Ok(job) => job,
            Err(error) => {
                warn!("Could not create the child-process job object: {error}");
                return None;
            }
        };

        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let result = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );

        if let Err(error) = result {
            // A job that does not kill on close is worse than none: children
            // would join it and still outlive us, and we would have logged
            // success.
            warn!("Could not arm the child-process job object: {error}");
            let _ = CloseHandle(job);
            return None;
        }

        debug!("Child processes will be terminated with this process");
        Some(job.0 as isize)
    }

    /// Tie `child` to this process, so it cannot outlive us.
    ///
    /// Best effort by design. Failure leaves the child running exactly as it
    /// did before this module existed, and the explicit shutdown path still
    /// covers a clean exit, so it must never stop the app from starting.
    pub fn attach_to_app_lifetime(child: &Child) {
        use std::os::windows::io::AsRawHandle;

        let Some(job) = job_handle() else {
            return;
        };

        // The handle `Child` already owns, rather than reopening by PID: the
        // PID could have been recycled between the spawn and this call, and
        // assigning a stranger's process to our kill-on-close job would end it.
        let process = HANDLE(child.as_raw_handle());
        if let Err(error) = unsafe { AssignProcessToJobObject(job, process) } {
            warn!(
                pid = child.id(),
                "Child will not be terminated with this process: {error}"
            );
        }
    }

    /// Terminate engines left behind by earlier runs of this app.
    ///
    /// Only reaches processes running `executable` - this install's own managed
    /// engine binary - whose parent is gone. Returns how many were ended.
    pub fn reap_orphans(executable: &Path) -> usize {
        let Some(file_name) = executable.file_name() else {
            return 0;
        };

        let processes = unsafe { snapshot(file_name.to_string_lossy().as_ref()) };
        let own_pid = std::process::id();
        let mut reaped = 0;

        for orphan in orphans(&processes, executable, own_pid) {
            if unsafe { terminate(orphan.pid, executable) } {
                info!(
                    pid = orphan.pid,
                    "Ended a translation engine left behind by an earlier run"
                );
                reaped += 1;
            }
        }

        reaped
    }

    /// Walk the process table, resolving full image paths only for processes
    /// whose file name could match. Opening a handle to every process on the
    /// machine to answer a question about one executable is not worth it.
    unsafe fn snapshot(file_name: &str) -> Vec<ProcessEntry> {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return Vec::new();
        };

        let mut entries = Vec::new();
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let name = String::from_utf16_lossy(&entry.szExeFile);
                let name = name.trim_end_matches('\0');
                entries.push(ProcessEntry {
                    pid: entry.th32ProcessID,
                    parent_pid: entry.th32ParentProcessID,
                    image_path: name
                        .eq_ignore_ascii_case(file_name)
                        .then(|| image_path(entry.th32ProcessID))
                        .flatten(),
                });

                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = CloseHandle(snapshot);
        entries
    }

    unsafe fn image_path(pid: u32) -> Option<PathBuf> {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let path = image_path_of(process);
        let _ = CloseHandle(process);
        path
    }

    unsafe fn image_path_of(process: HANDLE) -> Option<PathBuf> {
        let mut buffer = [0u16; 32768];
        let mut length = buffer.len() as u32;
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
        .ok()
        .map(|()| PathBuf::from(String::from_utf16_lossy(&buffer[..length as usize])))
    }

    /// End `pid`, but only while it is still the process the snapshot identified.
    ///
    /// The snapshot proved the identity; by the time we reach here the PID may
    /// name something else entirely. Startup is the busiest moment for process
    /// creation on the machine, and an engine that was still exiting when we
    /// looked can free its PID onto a WebView helper before we act on it.
    ///
    /// Re-reading the image path from the handle we are about to terminate
    /// closes that window rather than narrowing it: the handle pins the kernel
    /// object, so a PID recycled after the open cannot alias what we hold. The
    /// spawn side already reasons this way - see `attach_to_app_lifetime`, which
    /// uses the handle `Child` owns instead of reopening by PID - and this is
    /// the more dangerous of the two, because it ends a process rather than
    /// enrolling one.
    unsafe fn terminate(pid: u32, expected: &Path) -> bool {
        let Ok(process) = OpenProcess(
            PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
            false,
            pid,
        ) else {
            return false;
        };

        let still_ours =
            image_path_of(process).is_some_and(|path| normalize(&path) == normalize(expected));
        let ended = still_ours && TerminateProcess(process, 1).is_ok();
        let _ = CloseHandle(process);
        ended
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Dry run of the reaper against the machine's real process table.
        ///
        /// The unit tests above feed `orphans` a table we wrote, so they say
        /// nothing about whether the Win32 walk fills that table correctly.
        /// This kills nothing; it reports what a real launch would end.
        ///
        /// The identity re-check in `terminate`, against a real process.
        ///
        /// A PID on its own is not proof of anything: between the snapshot that
        /// identified a process and the call that ends it, the number can come
        /// to mean something else. Spawning a process we own and asking
        /// `terminate` to end it under the wrong name is the only way to show
        /// the guard actually refuses.
        #[test]
        fn a_process_that_is_not_the_engine_is_not_terminated() {
            let mut victim = std::process::Command::new("ping")
                .args(["-n", "10", "127.0.0.1"])
                .stdout(std::process::Stdio::null())
                .spawn()
                .expect("the fixture process should start");
            let pid = victim.id();

            let refused = !unsafe { terminate(pid, Path::new(r"C:\nowhere\llama-server.exe")) };
            let survived = victim
                .try_wait()
                .expect("polling the fixture should succeed")
                .is_none();

            // Ending it by its true path both cleans up and proves the guard
            // rejects on identity rather than refusing everything.
            let actual = unsafe { image_path(pid) }.expect("the fixture path should resolve");
            let ended = unsafe { terminate(pid, &actual) };
            let _ = victim.wait();

            assert!(refused, "a mismatched path must not terminate");
            assert!(survived, "the process must outlive a mismatched path");
            assert!(ended, "a matching path must still terminate");
        }

        /// Dry run of the reaper against the machine's real process table.
        ///
        /// `MEOWCAL_REAP_DRY_RUN=<full path to llama-server.exe> cargo test
        /// -- --ignored --nocapture reports_what_a_real_launch_would_reap`
        #[test]
        #[ignore = "reads the live process table; run explicitly"]
        fn reports_what_a_real_launch_would_reap() {
            let Ok(executable) = std::env::var("MEOWCAL_REAP_DRY_RUN") else {
                panic!("set MEOWCAL_REAP_DRY_RUN to the managed engine's full path");
            };
            let executable = PathBuf::from(executable);
            let file_name = executable
                .file_name()
                .expect("the path must name a file")
                .to_string_lossy()
                .into_owned();

            let processes = unsafe { snapshot(&file_name) };
            println!("{} processes on this machine", processes.len());
            for process in processes.iter().filter(|p| p.image_path.is_some()) {
                println!(
                    "  pid {} parent {} -> {:?}",
                    process.pid, process.parent_pid, process.image_path
                );
            }
            let reap: Vec<_> = orphans(&processes, &executable, std::process::id())
                .iter()
                .map(|process| process.pid)
                .collect();
            println!("would reap: {reap:?}");

            // The walk itself has to work, whatever it found: a snapshot that
            // cannot see this test's own process is not reporting the machine.
            assert!(processes.iter().any(|p| p.pid == std::process::id()));
        }
    }
}

#[cfg(target_os = "windows")]
pub use windows_impl::{attach_to_app_lifetime, reap_orphans};
