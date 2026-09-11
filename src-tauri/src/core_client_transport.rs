#[path = "core_client_stderr.rs"]
mod diagnostics;

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

const MAX_REQUEST_BYTES: usize = meowcal_core::protocol::MAX_FRAME_BYTES;
const MAX_RESPONSE_BYTES: usize = meowcal_core::protocol::MAX_FRAME_BYTES;

#[derive(Debug)]
pub(super) enum Failure {
    Remote { code: String, message: String },
    Fatal(String),
}

#[derive(Debug)]
enum ReaderEvent {
    Frame(String),
    Failed(String),
    Eof,
}

pub(super) struct Transport {
    child: Child,
    stdin: ChildStdin,
    frames: mpsc::Receiver<ReaderEvent>,
    next_id: u64,
    kill_switch: Arc<KillSwitch>,
    diagnostics: Option<std::thread::JoinHandle<()>>,
}

pub(super) struct KillSwitch {
    handle: isize,
    killed: AtomicBool,
}

unsafe impl Send for KillSwitch {}
unsafe impl Sync for KillSwitch {}

impl KillSwitch {
    #[cfg(target_os = "windows")]
    fn for_child(child: &Child) -> Result<Arc<Self>, String> {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::Foundation::{DuplicateHandle, DUPLICATE_SAME_ACCESS, HANDLE};
        use windows::Win32::System::Threading::GetCurrentProcess;
        let process = unsafe { GetCurrentProcess() };
        let mut duplicated = HANDLE::default();
        unsafe {
            DuplicateHandle(
                process,
                HANDLE(child.as_raw_handle()),
                process,
                &mut duplicated,
                0,
                false,
                DUPLICATE_SAME_ACCESS,
            )
        }
        .map_err(|error| format!("CORE_PROCESS_HANDLE: {error}"))?;
        Ok(Arc::new(Self {
            handle: duplicated.0 as isize,
            killed: AtomicBool::new(false),
        }))
    }

    #[cfg(target_os = "windows")]
    pub(super) fn kill(&self) {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::Threading::{TerminateProcess, WaitForSingleObject};
        if !self.killed.swap(true, Ordering::SeqCst) {
            let _ = unsafe { TerminateProcess(HANDLE(self.handle as *mut std::ffi::c_void), 1) };
        }
        let _ = unsafe { WaitForSingleObject(HANDLE(self.handle as *mut std::ffi::c_void), 5_000) };
    }
}

#[cfg(target_os = "windows")]
impl Drop for KillSwitch {
    fn drop(&mut self) {
        use windows::Win32::Foundation::{CloseHandle, HANDLE};
        let _ = unsafe { CloseHandle(HANDLE(self.handle as *mut std::ffi::c_void)) };
    }
}

impl Transport {
    pub(super) fn spawn(executable: &Path) -> Result<Self, String> {
        Self::spawn_command(crate::windowless_command::std_command(executable))
    }

    fn spawn_command(mut command: std::process::Command) -> Result<Self, String> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| format!("CORE_START_FAILED: {error}"))?;
        if let Err(error) = crate::process_lifetime::attach_to_app_lifetime(&child) {
            terminate_child(&mut child);
            return Err(error);
        }
        let kill_switch =
            KillSwitch::for_child(&child).inspect_err(|_| terminate_child(&mut child))?;
        let stdin = match child.stdin.take() {
            Some(stdin) => stdin,
            None => {
                kill_switch.kill();
                terminate_child(&mut child);
                return Err("CORE_STDIN_MISSING".to_string());
            }
        };
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                kill_switch.kill();
                terminate_child(&mut child);
                return Err("CORE_STDOUT_MISSING".to_string());
            }
        };
        let (sender, frames) = mpsc::sync_channel(16);
        if let Err(error) = std::thread::Builder::new()
            .name("meowcal-core-reader".to_string())
            .spawn(move || read_frames(BufReader::new(stdout), sender))
        {
            kill_switch.kill();
            terminate_child(&mut child);
            return Err(format!("CORE_READER_START: {error}"));
        }
        let diagnostics = diagnostics::spawn(&mut child).inspect_err(|_| {
            kill_switch.kill();
            terminate_child(&mut child);
        })?;
        Ok(Self {
            child,
            stdin,
            frames,
            next_id: 1,
            kill_switch,
            diagnostics: Some(diagnostics),
        })
    }

    pub(super) fn request(
        &mut self,
        method: &str,
        params: impl Into<super::Request>,
        timeout: Duration,
        progress: Option<&(dyn Fn(String) + Send + Sync)>,
        cancelled: Option<&Arc<AtomicBool>>,
    ) -> Result<Value, Failure> {
        let request = params.into();
        let id = self.next_id;
        self.next_id += 1;
        let frame = serde_json::to_vec(
            &json!({"id":id,"api":super::API_VERSION,"method":method,"params":request.params,"payloadBytes":request.payload.len()}),
        )
        .map_err(|error| Failure::Fatal(format!("CORE_REQUEST_INVALID: {error}")))?;
        if frame.len() + 1 > MAX_REQUEST_BYTES
            || request.payload.len() > meowcal_core::ocr::MAX_FRAME_BYTES
        {
            return Err(Failure::Fatal("CORE_REQUEST_TOO_LARGE".to_string()));
        }
        let deadline = Instant::now() + timeout;
        let done = Arc::new(AtomicBool::new(false));
        let _watchdog = WatchdogDone(done.clone());
        let watchdog_cancelled = cancelled.cloned();
        let watchdog_kill = self.kill_switch.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("meowcal-core-watchdog".to_string())
            .spawn(move || {
                while !done.load(Ordering::SeqCst) {
                    let cancelled = watchdog_cancelled
                        .as_ref()
                        .is_some_and(|flag| flag.load(Ordering::SeqCst));
                    if cancelled || Instant::now() >= deadline {
                        watchdog_kill.kill();
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            })
        {
            self.kill_switch.kill();
            return Err(Failure::Fatal(format!("CORE_WATCHDOG_START: {error}")));
        }
        self.stdin
            .write_all(&frame)
            .and_then(|_| self.stdin.write_all(b"\n"))
            .and_then(|_| self.stdin.write_all(&request.payload))
            .and_then(|_| self.stdin.flush())
            .map_err(|error| Failure::Fatal(format!("CORE_STDIN_WRITE: {error}")))?;
        loop {
            if cancelled.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
                return Err(Failure::Fatal("CORE_REQUEST_CANCELLED".to_string()));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(Failure::Fatal(format!("CORE_REQUEST_TIMEOUT: {method}")));
            }
            match self
                .frames
                .recv_timeout(remaining.min(Duration::from_millis(50)))
            {
                Ok(ReaderEvent::Frame(frame)) => {
                    let value: Value = serde_json::from_str(&frame).map_err(|error| {
                        Failure::Fatal(format!("CORE_PROTOCOL_INVALID_JSON: {error}"))
                    })?;
                    if value.get("id").and_then(Value::as_u64) != Some(id) {
                        return Err(Failure::Fatal("CORE_PROTOCOL_UNEXPECTED_ID".to_string()));
                    }
                    if value.get("event").and_then(Value::as_str) == Some("progress") {
                        let message =
                            value
                                .get("message")
                                .and_then(Value::as_str)
                                .ok_or_else(|| {
                                    Failure::Fatal("CORE_PROTOCOL_INVALID_PROGRESS".to_string())
                                })?;
                        if let Some(callback) = progress {
                            callback(message.to_string());
                        }
                        continue;
                    }
                    match (value.get("result"), value.get("error")) {
                        (Some(result), None) => return Ok(result.clone()),
                        (None, Some(error)) => {
                            let code =
                                error.get("code").and_then(Value::as_str).ok_or_else(|| {
                                    Failure::Fatal("CORE_PROTOCOL_INVALID_ERROR".to_string())
                                })?;
                            let message =
                                error
                                    .get("message")
                                    .and_then(Value::as_str)
                                    .ok_or_else(|| {
                                        Failure::Fatal("CORE_PROTOCOL_INVALID_ERROR".to_string())
                                    })?;
                            return Err(Failure::Remote {
                                code: code.to_string(),
                                message: message.to_string(),
                            });
                        }
                        _ => {
                            return Err(Failure::Fatal(
                                "CORE_PROTOCOL_INVALID_RESPONSE".to_string(),
                            ))
                        }
                    }
                }
                Ok(ReaderEvent::Failed(error)) => return Err(Failure::Fatal(error)),
                Ok(ReaderEvent::Eof) => return Err(Failure::Fatal("CORE_STDOUT_EOF".to_string())),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(Failure::Fatal("CORE_READER_STOPPED".to_string()))
                }
            }
        }
    }

    pub(super) fn kill_switch(&self) -> Arc<KillSwitch> {
        self.kill_switch.clone()
    }

    pub(super) fn has_exited(&mut self) -> Result<bool, String> {
        self.child
            .try_wait()
            .map(|status| status.is_some())
            .map_err(|error| format!("CORE_PROCESS_STATUS: {error}"))
    }

    pub(super) fn kill_and_wait(&mut self) {
        self.kill_switch.kill();
        terminate_child(&mut self.child);
        if let Some(reader) = self.diagnostics.take() {
            if reader.join().is_err() {
                tracing::warn!(core_pid = self.pid(), "Core stderr reader panicked");
            }
        }
    }

    pub(super) fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        self.kill_and_wait();
    }
}

struct WatchdogDone(Arc<AtomicBool>);
impl Drop for WatchdogDone {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn read_frames<R: BufRead>(mut reader: R, sender: mpsc::SyncSender<ReaderEvent>) {
    loop {
        match read_frame(&mut reader) {
            Ok(None) => {
                let _ = sender.send(ReaderEvent::Eof);
                return;
            }
            Ok(Some(mut bytes)) => {
                while matches!(bytes.last(), Some(b'\n' | b'\r')) {
                    bytes.pop();
                }
                match String::from_utf8(bytes) {
                    Ok(frame) => {
                        if sender.send(ReaderEvent::Frame(frame)).is_err() {
                            return;
                        }
                    }
                    Err(_) => {
                        let _ =
                            sender.send(ReaderEvent::Failed("CORE_RESPONSE_NOT_UTF8".to_string()));
                        return;
                    }
                }
            }
            Err(error) => {
                let _ = sender.send(ReaderEvent::Failed(error));
                return;
            }
        }
    }
}

fn read_frame<R: BufRead>(reader: &mut R) -> Result<Option<Vec<u8>>, String> {
    let mut frame = Vec::new();
    loop {
        let available = reader
            .fill_buf()
            .map_err(|error| format!("CORE_STDOUT_READ: {error}"))?;
        if available.is_empty() {
            return if frame.is_empty() {
                Ok(None)
            } else {
                Err("CORE_PROTOCOL_UNTERMINATED_FRAME".to_string())
            };
        }
        let take = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        if frame.len().saturating_add(take) > MAX_RESPONSE_BYTES {
            return Err("CORE_RESPONSE_TOO_LARGE".to_string());
        }
        frame.extend_from_slice(&available[..take]);
        reader.consume(take);
        if frame.last() == Some(&b'\n') {
            return Ok(Some(frame));
        }
    }
}

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
#[path = "core_client_transport_tests.rs"]
mod tests;
