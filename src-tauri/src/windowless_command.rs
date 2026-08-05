// =============================================================================
// WINDOWLESS_COMMAND.RS - subprocesses that do not flash a console
// =============================================================================
// Issue #67: the app spawns helper processes while the viewer is watching
// full-screen video, and on Windows a console subprocess draws a window unless
// told not to. Four `foundry` probes and the setup extraction omitted the flag,
// so black console windows flashed over playback - and where a stale Windows
// Store execution alias stood in for `foundry.exe`, the same spawns raised a
// modal `0xc0000142` error dialog on top of it.
//
// The flag itself was never the hard part; it was already correct in three
// places and simply absent in five others, with `0x08000000` written out by
// hand each time. Routing every spawn through here is what stops the next one
// from forgetting, and is why these return a builder rather than run anything:
// callers still choose their own args, stdio and whether to wait.
// =============================================================================

/// `CREATE_NO_WINDOW` - suppresses the console a child process would otherwise
/// be given. Defined once here so no call site has to remember the number.
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A `std::process::Command` that will not draw a console window.
pub fn std_command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// A `tokio::process::Command` that will not draw a console window.
///
/// The async twin of `std_command`, for the setup path, which extracts through
/// PowerShell and must not block the runtime while it does.
pub fn tokio_command(program: impl AsRef<std::ffi::OsStr>) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(program);
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    // Not a behavioural assertion - `creation_flags` is write-only, so there is
    // nothing to read back. This pins that both builders exist and run, which is
    // what stops a refactor from quietly dropping one of them.
    #[test]
    fn both_builders_produce_a_runnable_command() {
        let mut std_probe = std_command("cmd");
        std_probe.args(["/C", "exit 0"]);
        assert!(std_probe.status().is_ok());

        let tokio_probe = tokio_command("cmd");
        assert_eq!(tokio_probe.as_std().get_program(), "cmd");
    }
}
