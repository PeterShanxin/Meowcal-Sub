#[path = "core_client_async.rs"]
mod async_call;
#[path = "core_client_config.rs"]
mod config;
#[path = "core_client_request.rs"]
mod request;
#[path = "core_client_transport.rs"]
mod transport;
#[path = "core_client_types.rs"]
mod types;

use async_call::call_async;
pub use config::{configure_storage, register, register_headless, select_storage_root};
use request::call;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use transport::{Failure, KillSwitch, Transport};
pub use types::{CoreInstallPaths, CoreStatus, OcrRecognizeParams, OcrRecognizeResult, StatusPoll};
use types::{HelloResult, OcrInitializeResult, OcrLanguagesResult, Request};

pub(super) const API_VERSION: u32 = meowcal_core::protocol::API_VERSION;
const CORE_VERSION: &str = env!("MEOWCAL_EXPECTED_CORE_VERSION");
#[cfg(meowcal_core_source_candidate)]
pub(super) const CORE_SOURCE_CANDIDATE: bool = true;
#[cfg(not(meowcal_core_source_candidate))]
pub(super) const CORE_SOURCE_CANDIDATE: bool = false;
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
pub const READY_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone)]
struct LaunchConfig {
    executable: PathBuf,
    profile: &'static str,
    storage_root: Option<PathBuf>,
    legacy_roots: Vec<PathBuf>,
}

static CONFIG: OnceLock<Mutex<Option<LaunchConfig>>> = OnceLock::new();
static TRANSLATION: OnceLock<Mutex<Option<Transport>>> = OnceLock::new();
static OCR: OnceLock<Mutex<Option<Transport>>> = OnceLock::new();
static TRANSLATION_KILL: OnceLock<Mutex<Option<Arc<KillSwitch>>>> = OnceLock::new();
static OCR_KILL: OnceLock<Mutex<Option<Arc<KillSwitch>>>> = OnceLock::new();
static STATUS: Mutex<Option<CoreStatus>> = Mutex::new(None);

pub fn cached_status() -> Option<CoreStatus> {
    STATUS.lock().ok().and_then(|status| status.clone())
}

pub fn status_blocking() -> Result<CoreStatus, String> {
    call(
        &TRANSLATION,
        "status",
        json!({}),
        Duration::from_secs(5),
        None,
        None,
        false,
    )
}

fn invalidate_readiness() {
    if let Ok(mut status) = STATUS.lock() {
        if let Some(status) = status.as_mut() {
            status.ready = false;
        }
    }
}

pub async fn status() -> Result<CoreStatus, String> {
    call_async(
        &TRANSLATION,
        "status",
        json!({}),
        Duration::from_secs(5),
        None,
        false,
    )
    .await
}

pub async fn status_if_idle() -> Result<StatusPoll, String> {
    tokio::task::spawn_blocking(|| {
        request::poll_status(
            TRANSLATION.get_or_init(|| Mutex::new(None)),
            &TRANSLATION_KILL,
            Duration::from_secs(5),
        )
    })
    .await
    .map_err(|error| format!("CORE_TASK_FAILED: {error}"))?
}

pub async fn install(progress: Arc<dyn Fn(String) + Send + Sync>) -> Result<CoreStatus, String> {
    call_async(
        &TRANSLATION,
        "install",
        json!({}),
        Duration::from_secs(30 * 60 + 15),
        Some(progress),
        false,
    )
    .await
}

pub fn ready_blocking(timeout: Duration) -> Result<CoreStatus, String> {
    call(&TRANSLATION, "ready", json!({}), timeout, None, None, false)
}

pub async fn ready(timeout: Duration) -> Result<CoreStatus, String> {
    call_async(&TRANSLATION, "ready", json!({}), timeout, None, false).await
}

pub async fn complete(request: Value, timeout_ms: u64) -> Result<Value, String> {
    let timeout_ms = timeout_ms.clamp(1, 90_000);
    call_async(
        &TRANSLATION,
        "complete",
        json!({"request":request,"timeoutMs":timeout_ms}),
        Duration::from_millis(timeout_ms + 2_000),
        None,
        true,
    )
    .await
}

pub fn ocr_languages_blocking() -> Result<Vec<String>, String> {
    call::<OcrLanguagesResult>(
        &OCR,
        "ocrLanguages",
        json!({}),
        Duration::from_secs(5),
        None,
        None,
        false,
    )
    .map(|result| result.languages)
}

pub fn ocr_initialize_blocking(language: Option<String>) -> Result<String, String> {
    call::<OcrInitializeResult>(
        &OCR,
        "ocrInitialize",
        json!({"language":language}),
        Duration::from_secs(5),
        None,
        None,
        false,
    )
    .map(|result| result.resolved_language)
}

pub async fn ocr_recognize(
    params: OcrRecognizeParams,
    payload: Vec<u8>,
) -> Result<OcrRecognizeResult, String> {
    let timeout = Duration::from_millis(params.timeout_ms.min(30_000) + 2_000);
    call_async(
        &OCR,
        "ocrRecognizeBgra",
        Request::ocr(params, payload)?,
        timeout,
        None,
        false,
    )
    .await
}

pub fn owned_pid() -> Option<u32> {
    TRANSLATION
        .get()
        .and_then(|slot| slot.lock().ok())
        .and_then(|slot| slot.as_ref().map(Transport::pid))
}

pub fn shutdown_owned() {
    invalidate_readiness();
    shutdown_slot(&TRANSLATION, &TRANSLATION_KILL);
    shutdown_slot(&OCR, &OCR_KILL);
}

fn spawn_initialized(
    kill_slot: &OnceLock<Mutex<Option<Arc<KillSwitch>>>>,
    deadline: Instant,
    method: &str,
    cancelled: Option<&Arc<AtomicBool>>,
) -> Result<Transport, String> {
    let config = CONFIG
        .get()
        .and_then(|value| value.lock().ok())
        .and_then(|value| value.clone())
        .ok_or_else(|| "CORE_NOT_REGISTERED".to_string())?;
    let mut process = Transport::spawn(&config.executable)?;
    *kill_slot
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "CORE_KILL_LOCK_POISONED".to_string())? = Some(process.kill_switch());
    if cancelled.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
        process.kill_and_wait();
        clear_kill(kill_slot);
        return Err("CORE_REQUEST_CANCELLED".to_string());
    }
    let hello = json!({"client":"sub1","profile":config.profile,"expectedVersion":CORE_VERSION,"storageRoot":config.storage_root,"legacyRoots":config.legacy_roots});
    let hello_timeout = match remaining(deadline, method) {
        Ok(remaining) => remaining.min(HELLO_TIMEOUT),
        Err(error) => {
            process.kill_and_wait();
            clear_kill(kill_slot);
            return Err(error);
        }
    };
    let result = process
        .request("hello", hello, hello_timeout, None, cancelled)
        .map_err(failure_message);
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            process.kill_and_wait();
            clear_kill(kill_slot);
            return Err(error);
        }
    };
    let hello: HelloResult = serde_json::from_value(result).map_err(|error| {
        process.kill_and_wait();
        clear_kill(kill_slot);
        format!("CORE_HELLO_INVALID: {error}")
    })?;
    if !hello_is_compatible(&hello, CORE_VERSION) {
        process.kill_and_wait();
        clear_kill(kill_slot);
        return Err(format!("CORE_INCOMPATIBLE: expected version {CORE_VERSION}, API {API_VERSION}, and required capabilities"));
    }
    Ok(process)
}

fn hello_is_compatible(hello: &HelloResult, expected_version: &str) -> bool {
    hello.version == expected_version
        && hello.api == API_VERSION
        && meowcal_core::protocol::CAPABILITIES
            .iter()
            .all(|required| hello.capabilities.iter().any(|actual| actual == required))
}

fn remaining(deadline: Instant, method: &str) -> Result<Duration, String> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        Err(format!("CORE_REQUEST_TIMEOUT: {method}"))
    } else {
        Ok(remaining)
    }
}

fn shutdown_slot(
    slot: &'static OnceLock<Mutex<Option<Transport>>>,
    kill_slot: &OnceLock<Mutex<Option<Arc<KillSwitch>>>>,
) {
    let Some(slot) = slot.get() else { return };
    let Ok(mut guard) = slot.try_lock() else {
        kill_current(kill_slot);
        return;
    };
    if let Some(process) = guard.as_mut() {
        let _ = process.request("shutdown", json!({}), Duration::from_secs(2), None, None);
        process.kill_and_wait();
    }
    *guard = None;
    clear_kill(kill_slot);
}

fn clear_process(
    guard: &mut Option<Transport>,
    kill_slot: &OnceLock<Mutex<Option<Arc<KillSwitch>>>>,
) {
    if std::ptr::eq(kill_slot, &TRANSLATION_KILL) {
        invalidate_readiness();
    }
    if let Some(process) = guard.as_mut() {
        process.kill_and_wait();
    }
    *guard = None;
    clear_kill(kill_slot);
}

fn kill_slot_for(
    slot: &'static OnceLock<Mutex<Option<Transport>>>,
) -> &'static OnceLock<Mutex<Option<Arc<KillSwitch>>>> {
    if std::ptr::eq(slot, &TRANSLATION) {
        &TRANSLATION_KILL
    } else {
        &OCR_KILL
    }
}

fn fatal_remote(code: &str) -> bool {
    matches!(
        code,
        "OCR_TIMEOUT" | "INSTALL_TIMEOUT" | "READY_TIMEOUT" | "OCR_PROCESS_UNUSABLE"
    )
}

fn failure_message(error: Failure) -> String {
    match error {
        Failure::Remote { code, message } => format!("CORE_{code}: {message}"),
        Failure::Fatal(message) => message,
    }
}

fn kill_current(slot: &OnceLock<Mutex<Option<Arc<KillSwitch>>>>) {
    if let Some(kill) = slot
        .get()
        .and_then(|slot| slot.lock().ok())
        .and_then(|slot| slot.clone())
    {
        kill.kill();
    }
}

fn clear_kill(slot: &OnceLock<Mutex<Option<Arc<KillSwitch>>>>) {
    if let Some(slot) = slot.get() {
        if let Ok(mut slot) = slot.lock() {
            *slot = None;
        }
    }
}

#[cfg(test)]
#[path = "core_client_tests.rs"]
mod tests;
