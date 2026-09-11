use super::{
    clear_process, fatal_remote, kill_slot_for, remaining, spawn_initialized, Failure, KillSwitch,
    Request, StatusPoll, Transport,
};
use serde::de::DeserializeOwned;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

struct CallOptions<'a> {
    method: &'a str,
    deadline: Instant,
    progress: Option<&'a (dyn Fn(String) + Send + Sync)>,
    cancelled: Option<&'a Arc<AtomicBool>>,
    drain_active_on_drop: bool,
}

pub(super) fn call<T: DeserializeOwned>(
    slot: &'static OnceLock<Mutex<Option<Transport>>>,
    method: &str,
    params: impl Into<Request>,
    timeout: Duration,
    progress: Option<&(dyn Fn(String) + Send + Sync)>,
    cancelled: Option<&Arc<AtomicBool>>,
    drain_active_on_drop: bool,
) -> Result<T, String> {
    let deadline = Instant::now() + timeout;
    let mut guard = acquire_slot(
        slot.get_or_init(|| Mutex::new(None)),
        deadline,
        method,
        cancelled,
    )?;
    call_locked(
        &mut guard,
        kill_slot_for(slot),
        params.into(),
        CallOptions {
            method,
            deadline,
            progress,
            cancelled,
            drain_active_on_drop,
        },
    )
}

pub(super) fn poll_status(
    slot: &Mutex<Option<Transport>>,
    kill_slot: &OnceLock<Mutex<Option<Arc<KillSwitch>>>>,
    timeout: Duration,
) -> Result<StatusPoll, String> {
    let mut guard = match slot.try_lock() {
        Ok(guard) => guard,
        Err(std::sync::TryLockError::WouldBlock) => return Ok(StatusPoll::Busy),
        Err(std::sync::TryLockError::Poisoned(_)) => {
            return Err("CORE_PROCESS_LOCK_POISONED".to_string())
        }
    };
    call_locked(
        &mut guard,
        kill_slot,
        json!({}).into(),
        CallOptions {
            method: "status",
            deadline: Instant::now() + timeout,
            progress: None,
            cancelled: None,
            drain_active_on_drop: false,
        },
    )
    .map(|status| StatusPoll::Status(Box::new(status)))
}

fn call_locked<T: DeserializeOwned>(
    guard: &mut Option<Transport>,
    kill_slot: &OnceLock<Mutex<Option<Arc<KillSwitch>>>>,
    params: Request,
    options: CallOptions<'_>,
) -> Result<T, String> {
    let CallOptions {
        method,
        deadline,
        progress,
        cancelled,
        drain_active_on_drop,
    } = options;
    if cancelled.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
        return Err("CORE_REQUEST_CANCELLED".to_string());
    }
    if let Some(process) = guard.as_mut() {
        match process.has_exited() {
            Ok(true) => clear_process(guard, kill_slot),
            Ok(false) => {}
            Err(error) => {
                clear_process(guard, kill_slot);
                return Err(error);
            }
        }
    }
    if guard.is_none() {
        *guard = Some(spawn_initialized(kill_slot, deadline, method, cancelled)?);
    }
    let request_timeout = remaining(deadline, method)?;
    let process = guard
        .as_mut()
        .ok_or_else(|| "CORE_NOT_RUNNING".to_string())?;
    if cancelled.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
        return Err("CORE_REQUEST_CANCELLED".to_string());
    }
    let request_cancelled = if drain_active_on_drop {
        None
    } else {
        cancelled
    };
    let response = process.request(method, params, request_timeout, progress, request_cancelled);
    let value = match response {
        Ok(value) => value,
        Err(Failure::Remote { code, message }) if !fatal_remote(&code) => {
            return Err(format!("CORE_{code}: {message}"))
        }
        Err(Failure::Remote { code, message }) => {
            clear_process(guard, kill_slot);
            return Err(format!("CORE_{code}: {message}"));
        }
        Err(Failure::Fatal(message)) => {
            clear_process(guard, kill_slot);
            return Err(message);
        }
    };
    match serde_json::from_value(value) {
        Ok(result) => Ok(result),
        Err(error) => {
            clear_process(guard, kill_slot);
            Err(format!("CORE_RESULT_INVALID: {error}"))
        }
    }
}

pub(super) fn acquire_slot<'a>(
    mutex: &'a Mutex<Option<Transport>>,
    deadline: Instant,
    method: &str,
    cancelled: Option<&Arc<AtomicBool>>,
) -> Result<std::sync::MutexGuard<'a, Option<Transport>>, String> {
    loop {
        match mutex.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err("CORE_PROCESS_LOCK_POISONED".to_string())
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                if cancelled.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
                    return Err("CORE_REQUEST_CANCELLED".to_string());
                }
                if Instant::now() >= deadline {
                    return Err(format!("CORE_REQUEST_TIMEOUT: {method}"));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}
