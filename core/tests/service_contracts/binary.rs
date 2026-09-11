use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

struct Process {
    child: Arc<Mutex<Child>>,
    watchdog_stop: Option<mpsc::Sender<()>>,
    watchdog: Option<std::thread::JoinHandle<()>>,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
}

impl Process {
    fn new() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_meowcal-core"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let output = BufReader::new(child.stdout.take().unwrap());
        let child = Arc::new(Mutex::new(child));
        let watched_child = Arc::clone(&child);
        let (watchdog_stop, stop) = mpsc::channel();
        let watchdog = std::thread::spawn(move || {
            if matches!(
                stop.recv_timeout(Duration::from_secs(20)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ) {
                let mut child = watched_child.lock().unwrap();
                let _ = child.kill();
                let _ = child.wait();
            }
        });
        Self {
            child,
            watchdog_stop: Some(watchdog_stop),
            watchdog: Some(watchdog),
            input,
            output,
        }
    }

    fn send(&mut self, bytes: &[u8]) {
        let input = self.input.as_mut().unwrap();
        input.write_all(bytes).unwrap();
        input.flush().unwrap();
    }

    fn response(&mut self) -> Value {
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    fn exits(&mut self, budget: Duration) {
        let end = Instant::now() + budget;
        loop {
            if let Some(status) = self.child.lock().unwrap().try_wait().unwrap() {
                assert!(status.success());
                return;
            }
            assert!(Instant::now() < end, "Core retained a broken session");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        drop(self.input.take());
        drop(self.watchdog_stop.take());
        if let Some(watchdog) = self.watchdog.take() {
            let _ = watchdog.join();
        }
        let mut child = self.child.lock().unwrap();
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn header(payload_bytes: u64) -> Vec<u8> {
    format!(
        "{}\n",
        json!({"id":1,"api":1,"method":"ocrRecognizeBgra",
        "payloadBytes":payload_bytes,
        "params":{"language":null,"width":2,"height":1,"stride":8,"timeoutMs":1000}})
    )
    .into_bytes()
}

#[test]
fn binary_body_is_fragmented_and_never_parsed_as_json() {
    let mut process = Process::new();
    let mut frame = header(8);
    frame.extend_from_slice(b"\n\0{}\r\n\xff\0");
    for fragment in frame.chunks(3) {
        process.send(fragment);
    }
    // No hello is needed to prove the frame was consumed: dispatch must see
    // the complete request and return its ordinary initialization error.
    assert_eq!(process.response()["error"]["code"], "NOT_INITIALIZED");
    process.send(&header(8));
    process.send(&[0; 8]);
    assert_eq!(process.response()["error"]["code"], "NOT_INITIALIZED");
    process.send(b"{\"id\":2,\"api\":1,\"method\":\"shutdown\",\"params\":{}}\n");
    assert_eq!(process.response()["result"]["stopped"], true);
    process.exits(Duration::from_secs(3));
}

#[test]
fn invalid_binary_envelopes_close_without_waiting_for_the_body() {
    let mut cases = vec![header(7), header(64 * 1024 * 1024 + 1), header(0)];
    for change in [
        json!({"width":4097}),
        json!({"stride":12}),
        json!({"timeoutMs":0}),
        json!({"timeoutMs":30001}),
        json!({"bgraBase64":"AAAA"}),
    ] {
        let mut value: Value = serde_json::from_slice(&header(8)).unwrap();
        for (key, value_change) in change.as_object().unwrap() {
            value["params"][key] = value_change.clone();
        }
        cases.push(format!("{value}\n").into_bytes());
    }
    cases.push(
        b"{\"id\":1,\"api\":1,\"method\":\"status\",\"params\":{},\"payloadBytes\":8}\n".to_vec(),
    );
    cases.push(
        b"{\"id\":1,\"api\":1,\"method\":\"ocrRecognizeBgra\",\"params\":{},\"payloadBytes\":-1}\n"
            .to_vec(),
    );
    cases.push(vec![b'x'; meowcal_core::protocol::MAX_FRAME_BYTES + 1]);
    for bytes in cases {
        let mut process = Process::new();
        process.send(&bytes);
        process.exits(Duration::from_secs(3));
        let mut line = String::new();
        assert_eq!(process.output.read_line(&mut line).unwrap(), 0);
    }
}

#[test]
fn truncated_body_eof_and_deadline_end_the_session() {
    for close_stdin in [true, false] {
        let mut process = Process::new();
        process.send(&header(8));
        process.send(&[0, 10]);
        if close_stdin {
            drop(process.input.take());
        }
        process.exits(Duration::from_secs(if close_stdin { 3 } else { 8 }));
    }
}

#[test]
fn invalid_control_request_retains_structured_error_and_session() {
    let mut process = Process::new();
    process.send(b"{\"id\":1,\"api\":2,\"method\":\"status\",\"params\":{},\"payloadBytes\":0}\n");
    assert_eq!(process.response()["error"]["code"], "API_MISMATCH");
    process.send(b"{\"id\":2,\"api\":1,\"method\":\"shutdown\",\"params\":{}}\n");
    assert_eq!(process.response()["result"]["stopped"], true);
    process.exits(Duration::from_secs(3));
}

#[cfg(windows)]
#[test]
fn initialized_process_recognizes_binary_blank_frame_with_language_and_geometry() {
    let mut process = Process::new();
    let root = super::temporary_root("binary-ocr");
    for (id, method, params) in [
        (
            1,
            "hello",
            json!({"client":"sub2","profile":"development","expectedVersion":"0.1.0","storageRoot":root}),
        ),
        (2, "ocrLanguages", json!({})),
    ] {
        process.send(
            format!(
                "{}\n",
                json!({"id":id,"api":1,"method":method,"params":params})
            )
            .as_bytes(),
        );
        let reply = process.response();
        assert_eq!(reply["id"], id);
        assert!(reply.get("error").is_none(), "{reply}");
        if method == "ocrLanguages" {
            let language = reply["result"]["languages"][0].as_str().unwrap();
            process.send(
                format!(
                    "{}\n",
                    json!({"id":3,"api":1,"method":"ocrInitialize","params":{"language":language}})
                )
                .as_bytes(),
            );
            let initialized = process.response();
            assert_eq!(
                initialized["result"]["resolvedLanguage"], language,
                "{initialized}"
            );
            for id in [4, 5] {
                process.send(format!("{}\n", json!({"id":id,"api":1,"method":"ocrRecognizeBgra","payloadBytes":32000,
                    "params":{"language":language,"width":100,"height":80,"stride":400,"timeoutMs":5000}})).as_bytes());
                process.send(&[255; 32000]);
                let recognized = process.response();
                assert_eq!(recognized["id"], id);
                assert!(recognized.get("error").is_none(), "{recognized}");
                assert!(recognized["result"]["text"]
                    .as_str()
                    .unwrap()
                    .trim()
                    .is_empty());
                assert_eq!(recognized["result"]["frameWidth"], 100.0);
                assert_eq!(recognized["result"]["lines"], json!([]));
                assert_eq!(recognized["result"]["boxes"], json!([]));
            }
        }
    }
    process.send(b"{\"id\":6,\"api\":1,\"method\":\"shutdown\",\"params\":{}}\n");
    assert_eq!(process.response()["result"]["stopped"], true);
    process.exits(Duration::from_secs(3));
    assert!(!root.exists());
}
