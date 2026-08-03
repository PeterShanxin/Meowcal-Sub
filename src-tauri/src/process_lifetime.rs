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

use std::path::{Path, PathBuf};

/// A process as a snapshot of the process table reported it.
///
/// `image_path` is filled in only for processes worth identifying, so `None`
/// means "not a candidate", never "path unknown".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessEntry {
    pub pid: u32,
    pub parent_pid: u32,
    pub image_path: Option<PathBuf>,
}

/// Select the engine processes that have no owner left.
///
/// Deliberately conservative in one direction. A process whose parent PID has
/// been recycled onto some unrelated live process reads as owned and is left
/// alone, so a second Meowcal Sub running its own engine is never touched. The
/// cost is that an orphan is occasionally missed, which is a missed cleanup;
/// the opposite mistake would kill a working engine out from under a running
/// app. Ownership is proven by path, not by image name: only the executable
/// this install manages is a candidate, so an unrelated `llama-server.exe` is
/// not ours to end.
pub fn orphans<'a>(
    processes: &'a [ProcessEntry],
    executable: &Path,
    own_pid: u32,
) -> Vec<&'a ProcessEntry> {
    let target = normalize(executable);
    processes
        .iter()
        .filter(|process| process.pid != own_pid)
        .filter(|process| {
            process
                .image_path
                .as_deref()
                .is_some_and(|path| normalize(path) == target)
        })
        .filter(|process| !has_live_parent(process, processes))
        .collect()
}

/// PID 0 is the idle process and appears in every snapshot, so a process
/// reporting it as a parent has no real parent rather than a live one.
fn has_live_parent(process: &ProcessEntry, processes: &[ProcessEntry]) -> bool {
    process.parent_pid != 0
        && processes
            .iter()
            .any(|candidate| candidate.pid == process.parent_pid)
}

/// Compare paths the way Windows does: case-insensitively, and without letting
/// the extended-length prefix make two spellings of one file look different.
fn normalize(path: &Path) -> String {
    path.to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_lowercase()
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::{orphans, ProcessEntry};
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
            if unsafe { terminate(orphan.pid) } {
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
        let mut buffer = [0u16; 32768];
        let mut length = buffer.len() as u32;
        let queried = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut length,
        );
        let _ = CloseHandle(process);

        queried
            .ok()
            .map(|()| PathBuf::from(String::from_utf16_lossy(&buffer[..length as usize])))
    }

    unsafe fn terminate(pid: u32) -> bool {
        let Ok(process) = OpenProcess(PROCESS_TERMINATE, false, pid) else {
            return false;
        };
        let ended = TerminateProcess(process, 1).is_ok();
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

#[cfg(test)]
mod tests {
    use super::*;

    const OURS: &str = r"C:\Users\viewer\AppData\Local\meowcal-sub\runtime\llama-server.exe";
    const OWN_PID: u32 = 4242;

    fn engine(pid: u32, parent_pid: u32) -> ProcessEntry {
        ProcessEntry {
            pid,
            parent_pid,
            image_path: Some(PathBuf::from(OURS)),
        }
    }

    fn other(pid: u32) -> ProcessEntry {
        ProcessEntry {
            pid,
            parent_pid: 0,
            image_path: None,
        }
    }

    #[test]
    fn an_engine_whose_parent_is_gone_is_an_orphan() {
        let processes = vec![other(OWN_PID), engine(900, 800)];
        let found = orphans(&processes, Path::new(OURS), OWN_PID);
        assert_eq!(found, vec![&processes[1]]);
    }

    // The dangerous mistake this function can make: a second copy of the app is
    // running and its engine is working. Killing it would take the model out
    // from under a live session.
    #[test]
    fn an_engine_whose_parent_is_alive_is_left_alone() {
        let processes = vec![other(OWN_PID), other(800), engine(900, 800)];
        assert!(orphans(&processes, Path::new(OURS), OWN_PID).is_empty());
    }

    #[test]
    fn our_own_engine_is_left_alone() {
        let processes = vec![other(OWN_PID), engine(900, OWN_PID)];
        assert!(orphans(&processes, Path::new(OURS), OWN_PID).is_empty());
    }

    // Killing by image name is out of scope: someone else's llama-server is
    // not ours to end, however it got there.
    #[test]
    fn an_engine_from_another_install_is_not_ours_to_end() {
        let elsewhere = ProcessEntry {
            pid: 900,
            parent_pid: 0,
            image_path: Some(PathBuf::from(r"D:\tools\llama.cpp\llama-server.exe")),
        };
        let processes = vec![other(OWN_PID), elsewhere];
        assert!(orphans(&processes, Path::new(OURS), OWN_PID).is_empty());
    }

    #[test]
    fn a_reported_parent_of_zero_means_no_parent_rather_than_the_idle_process() {
        // PID 0 is in every snapshot, so treating it as a live parent would
        // hide the orphans that report it.
        let processes = vec![other(0), other(OWN_PID), engine(900, 0)];
        assert_eq!(
            orphans(&processes, Path::new(OURS), OWN_PID),
            vec![&processes[2]]
        );
    }

    #[test]
    fn path_spelling_does_not_decide_ownership() {
        let verbose = ProcessEntry {
            pid: 900,
            parent_pid: 0,
            image_path: Some(PathBuf::from(format!(r"\\?\{}", OURS.to_uppercase()))),
        };
        let processes = vec![other(OWN_PID), verbose];
        assert_eq!(orphans(&processes, Path::new(OURS), OWN_PID).len(), 1);
    }

    #[test]
    fn every_stranded_engine_is_collected_not_only_the_first() {
        let processes = vec![other(OWN_PID), engine(900, 800), engine(901, 801)];
        assert_eq!(orphans(&processes, Path::new(OURS), OWN_PID).len(), 2);
    }
}
