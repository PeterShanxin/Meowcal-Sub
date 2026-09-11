use super::*;
use std::io::Cursor;

#[test]
fn reader_accepts_one_newline_terminated_frame() {
    let mut input = Cursor::new(b"{\"id\":1}\r\n".to_vec());
    assert_eq!(
        read_frame(&mut input).expect("frame should parse"),
        Some(b"{\"id\":1}\r\n".to_vec())
    );
}

#[test]
fn reader_rejects_unterminated_and_oversized_frames() {
    let mut partial = Cursor::new(b"{\"id\":1}".to_vec());
    assert_eq!(
        read_frame(&mut partial).expect_err("partial frame must fail"),
        "CORE_PROTOCOL_UNTERMINATED_FRAME"
    );
    let mut oversized = Cursor::new(vec![b'x'; MAX_RESPONSE_BYTES + 1]);
    assert_eq!(
        read_frame(&mut oversized).expect_err("large frame must fail"),
        "CORE_RESPONSE_TOO_LARGE"
    );
}

#[test]
fn deadline_unblocks_a_writer_when_core_does_not_read() -> Result<(), String> {
    let powershell = Path::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
    let mut command = crate::windowless_command::std_command(powershell);
    command.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "Start-Sleep -Seconds 10",
    ]);
    let mut process = Transport::spawn_command(command)?;
    let started = Instant::now();
    let error = process
        .request(
            "hello",
            json!({"blob":"x".repeat(2 * 1024 * 1024)}),
            Duration::from_millis(150),
            None,
            None,
        )
        .expect_err("an interactive shell is not a Core server");
    process.kill_and_wait();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(matches!(error, Failure::Fatal(_)));
    assert!(process.has_exited()?);
    Ok(())
}

#[test]
fn cancellation_kills_the_owned_request() -> Result<(), String> {
    let powershell = Path::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
    let mut command = crate::windowless_command::std_command(powershell);
    command.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "Start-Sleep -Seconds 10",
    ]);
    let mut process = Transport::spawn_command(command)?;
    let cancelled = Arc::new(AtomicBool::new(true));
    let error = process
        .request(
            "status",
            json!({}),
            Duration::from_secs(2),
            None,
            Some(&cancelled),
        )
        .expect_err("cancelled request must stop");
    process.kill_and_wait();
    assert!(matches!(error, Failure::Fatal(message) if message == "CORE_REQUEST_CANCELLED"));
    assert!(process.has_exited()?);
    Ok(())
}

#[test]
fn malformed_handshake_frame_is_a_fatal_protocol_error() -> Result<(), String> {
    let powershell = Path::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
    let mut command = crate::windowless_command::std_command(powershell);
    command.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "[Console]::Out.WriteLine('not-json'); Start-Sleep -Seconds 10",
    ]);
    let mut process = Transport::spawn_command(command)?;
    let error = process
        .request("hello", json!({}), Duration::from_secs(2), None, None)
        .expect_err("malformed handshake must fail");
    process.kill_and_wait();
    assert!(
        matches!(error, Failure::Fatal(message) if message.starts_with("CORE_PROTOCOL_INVALID_JSON:"))
    );
    assert!(process.has_exited()?);
    Ok(())
}
