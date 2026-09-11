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
    let mut process = fixture("stall")?;
    let started = Instant::now();
    let error = process
        .request(
            "ocrRecognizeBgra",
            super::super::Request {
                params: json!({}),
                payload: vec![0; 2 * 1024 * 1024],
            },
            Duration::from_millis(150),
            None,
            None,
        )
        .expect_err("the fixture never reads the binary payload");
    process.kill_and_wait();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(
        matches!(&error, Failure::Fatal(message) if message.starts_with("CORE_STDIN_WRITE:")),
        "blocked payload writer returned {error:?}"
    );
    assert!(process.has_exited()?);
    Ok(())
}

#[test]
fn cancellation_kills_the_owned_request() -> Result<(), String> {
    let mut process = fixture("stall")?;
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
    let mut process = fixture("malformed")?;
    let error = process
        .request("hello", json!({}), Duration::from_secs(2), None, None)
        .expect_err("malformed handshake must fail");
    process.kill_and_wait();
    assert!(
        matches!(&error, Failure::Fatal(message) if message.starts_with("CORE_PROTOCOL_INVALID_JSON:")),
        "expected malformed JSON protocol error, received {error:?}"
    );
    assert!(process.has_exited()?);
    Ok(())
}

fn fixture(mode: &str) -> Result<Transport, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut command = crate::windowless_command::std_command(executable);
    command.args([
        "--exact",
        "core_client::transport::tests::pipe_fixture",
        "--nocapture",
    ]);
    command.env("MEOWCAL_SUB1_PIPE_FIXTURE", mode);
    let mut process = Transport::spawn_command(command)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match process
            .frames
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            Ok(ReaderEvent::Frame(frame)) if frame == "fixture-ready" => return Ok(process),
            Ok(ReaderEvent::Frame(_)) => continue,
            event => {
                let status = process.child.try_wait();
                return Err(format!(
                    "pipe fixture {mode} failed before request: {event:?}; child={status:?}"
                ));
            }
        }
    }
}

#[test]
fn pipe_fixture() {
    use std::io::Read;
    let Ok(mode) = std::env::var("MEOWCAL_SUB1_PIPE_FIXTURE") else {
        return;
    };
    let mut output = std::io::stdout().lock();
    writeln!(output, "\nfixture-ready").unwrap();
    output.flush().unwrap();
    if mode == "stall" {
        std::thread::sleep(Duration::from_secs(30));
        std::process::exit(0);
    }
    let mut input = BufReader::new(std::io::stdin().lock());
    let mut header = String::new();
    input.read_line(&mut header).unwrap();
    if mode == "malformed" {
        writeln!(output, "not-json").unwrap();
        output.flush().unwrap();
        std::thread::sleep(Duration::from_secs(30));
        std::process::exit(0);
    }
    if mode == "stderr" {
        let mut stderr = std::io::stderr().lock();
        stderr.write_all(&vec![b'x'; 128 * 1024]).unwrap();
        stderr.write_all(b"\nGPU failed; using CPU\n").unwrap();
        stderr.flush().unwrap();
        writeln!(
            output,
            "{{\"id\":1,\"result\":{{\"diagnosticsDrained\":true}}}}"
        )
        .unwrap();
        output.flush().unwrap();
        std::process::exit(0);
    }
    if mode == "status" || mode == "status-timeout" {
        let request: Value = serde_json::from_str(&header).unwrap();
        assert_eq!(request["id"], 1);
        assert_eq!(request["method"], "status");
        assert_eq!(request["payloadBytes"], 0);
        if mode == "status-timeout" {
            std::thread::sleep(Duration::from_secs(30));
        } else {
            writeln!(
                output,
                "{}",
                json!({"id":1,"result":{
                "installed":true,"ready":true,"model":"owned-model","version":"0.1.0",
                "storageRoot":"C:/core","managedConfig":null,"installPaths":null}})
            )
            .unwrap();
            output.flush().unwrap();
        }
        std::process::exit(0);
    }
    let header: Value = serde_json::from_str(&header).unwrap();
    assert_eq!(header["method"], "ocrRecognizeBgra");
    assert_eq!(header["payloadBytes"], 8);
    assert_eq!(
        header["params"],
        json!({"language":null,"width":2,"height":1,"stride":8,"timeoutMs":1000})
    );
    let mut payload = [0; 8];
    input.read_exact(&mut payload).unwrap();
    assert_eq!(payload, [0, 10, 13, 255, 123, 34, 0, 128]);
    writeln!(output, "{{\"id\":1,\"result\":{{}}}}").unwrap();
    output.flush().unwrap();
    let mut next = String::new();
    input.read_line(&mut next).unwrap();
    let next: Value = serde_json::from_str(&next).unwrap();
    assert_eq!(next["method"], "status");
    assert_eq!(next["payloadBytes"], 0);
    writeln!(output, "{{\"id\":2,\"result\":{{\"aligned\":true}}}}").unwrap();
    output.flush().unwrap();
    std::process::exit(0);
}

#[test]
fn binary_payload_preserves_bytes_and_next_request_alignment() -> Result<(), String> {
    let mut process = fixture("binary")?;
    let params = super::super::OcrRecognizeParams {
        language: None,
        width: 2,
        height: 1,
        stride: 8,
        timeout_ms: 1000,
    };
    let request = super::super::Request::ocr(params, vec![0, 10, 13, 255, 123, 34, 0, 128])?;
    process
        .request(
            "ocrRecognizeBgra",
            request,
            Duration::from_secs(2),
            None,
            None,
        )
        .map_err(|error| format!("binary request failed: {error:?}"))?;
    let result = process
        .request("status", json!({}), Duration::from_secs(2), None, None)
        .map_err(|error| format!("following control failed: {error:?}"))?;
    assert_eq!(result, json!({"aligned":true}));
    Ok(())
}

#[test]
fn stderr_is_drained_separately_and_reader_exits_with_core() -> Result<(), String> {
    let mut process = fixture("stderr")?;
    let result = process
        .request("hello", json!({}), Duration::from_secs(2), None, None)
        .map_err(|error| format!("stderr blocked protocol response: {error:?}"))?;
    assert_eq!(result, json!({"diagnosticsDrained":true}));
    process.kill_and_wait();
    assert!(process.has_exited()?);
    assert!(
        process.diagnostics.is_none(),
        "stderr reader must be joined"
    );
    Ok(())
}

#[test]
fn status_poll_busy_sends_nothing_and_idle_poll_uses_the_owned_process() -> Result<(), String> {
    use super::super::{request::poll_status, StatusPoll};
    use std::sync::{Mutex, OnceLock};
    let process = fixture("status")?;
    let pid = process.pid();
    let slot = Mutex::new(Some(process));
    let kill_slot = OnceLock::new();
    let mut active = slot.lock().unwrap();
    let started = Instant::now();
    assert!(matches!(
        poll_status(&slot, &kill_slot, Duration::from_secs(2))?,
        StatusPoll::Busy
    ));
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(!active.as_mut().unwrap().has_exited()?);
    assert_eq!(active.as_ref().unwrap().pid(), pid);
    drop(active);
    let StatusPoll::Status(status) = poll_status(&slot, &kill_slot, Duration::from_secs(2))? else {
        panic!("idle status poll must send the first request");
    };
    assert!(status.installed && status.ready);
    assert_eq!(status.model, "owned-model");
    Ok(())
}

#[test]
fn status_poll_response_deadline_is_an_error_and_reaps_the_process() -> Result<(), String> {
    use super::super::request::poll_status;
    use std::sync::{Mutex, OnceLock};
    let slot = Mutex::new(Some(fixture("status-timeout")?));
    let kill_slot = OnceLock::new();
    let started = Instant::now();
    let error = poll_status(&slot, &kill_slot, Duration::from_millis(150))
        .expect_err("a request already sent must retain its transport deadline");
    // The response reader and exact-process watchdog can observe the deadline
    // in either order; neither outcome is a busy status snapshot.
    assert!(
        matches!(
            error.as_str(),
            "CORE_REQUEST_TIMEOUT: status" | "CORE_STDOUT_EOF" | "CORE_READER_STOPPED"
        ),
        "{error}"
    );
    assert!(started.elapsed() >= Duration::from_millis(100));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(slot.lock().unwrap().is_none());
    Ok(())
}
