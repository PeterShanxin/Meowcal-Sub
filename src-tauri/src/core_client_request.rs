use super::{
    clear_process, fatal_remote, kill_slot_for, spawn_initialized, Failure, KillSwitch, Request,
    StatusPoll, Transport,
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
    deadline: Instant,
    progress: Option<&(dyn Fn(String) + Send + Sync)>,
    cancelled: Option<&Arc<AtomicBool>>,
    drain_active_on_drop: bool,
) -> Result<T, String> {
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
    mut params: Request,
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
    request_budget(deadline, method)?;
    if std::ptr::eq(kill_slot, &super::TRANSLATION_KILL)
        && method == "complete"
        && super::recovering()
    {
        return Err("CORE_INFERENCE_RECOVERING: Translation engine is recovering".into());
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
    let mut request_timeout = request_budget(deadline, method)?;
    if method == "complete" {
        params.params["timeoutMs"] = json!(request_timeout.as_millis().clamp(1, 90_000) as u64);
        // Drain the bounded Core response after caller expiry, preserving both
        // the warm process and response framing for the next request.
        request_timeout += Duration::from_secs(2);
    }
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
    let started_cpu_only = process.cpu_only;
    let on_progress = |message: String| {
        if std::ptr::eq(kill_slot, &super::TRANSLATION_KILL)
            && message == meowcal_core::inference_health::CPU_LOCK_EVENT
        {
            if super::cpu_lock_is_gpu_failure(true, started_cpu_only) {
                super::recovery::lock_cpu();
            }
        } else if let Some(progress) = progress {
            progress(message);
        }
    };
    let response = process.request(
        method,
        params,
        request_timeout,
        Some(&on_progress),
        request_cancelled,
    );
    let value = match response {
        Ok(value) => value,
        Err(Failure::Remote { code, message }) if !fatal_remote(&code) => {
            if std::ptr::eq(kill_slot, &super::TRANSLATION_KILL)
                && matches!(code.as_str(), "NOT_READY" | "TRANSPORT_ERROR")
            {
                super::invalidate_readiness();
            }
            return Err(format!("CORE_{code}: {message}"));
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
    if std::ptr::eq(kill_slot, &super::TRANSLATION_KILL)
        && matches!(method, "status" | "ready" | "install" | "recoverInference")
    {
        if let Ok(status) = serde_json::from_value::<super::CoreStatus>(value.clone()) {
            if super::cpu_lock_is_gpu_failure(status.cpu_locked, started_cpu_only) {
                super::recovery::lock_cpu();
            }
            if let Ok(mut cached) = super::STATUS.lock() {
                *cached = Some(status);
            }
        }
    }
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
                    return Err(expired(method));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

fn expired(method: &str) -> String {
    if method == "complete" {
        "CORE_COMPLETION_TIMEOUT: request deadline expired".into()
    } else {
        format!("CORE_REQUEST_TIMEOUT: {method}")
    }
}

fn request_budget(deadline: Instant, method: &str) -> Result<Duration, String> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        Err(expired(method))
    } else {
        Ok(left)
    }
}
