// =============================================================================
// PROCESS OWNERSHIP - which engine processes are ours to end
// =============================================================================
// The decision half of `process_lifetime`, kept away from the Win32 calls that
// gather the evidence and act on it. Deciding whether to terminate a process is
// the part that has to be right, and here it is a pure function over a list -
// testable without a process table, a job object, or Windows.
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
pub(crate) fn normalize(path: &Path) -> String {
    path.to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_lowercase()
}

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
