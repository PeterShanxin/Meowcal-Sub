use serde::de::DeserializeOwned;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use super::Transport;

pub(super) fn call_async<T: DeserializeOwned + Send + 'static>(
    slot: &'static OnceLock<Mutex<Option<Transport>>>,
    method: &'static str,
    params: Value,
    timeout: Duration,
    progress: Option<Arc<dyn Fn(String) + Send + Sync>>,
    drain_active_on_drop: bool,
) -> impl std::future::Future<Output = Result<T, String>> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = cancelled.clone();
    let task = tokio::task::spawn_blocking(move || {
        super::call(
            slot,
            method,
            params,
            timeout,
            progress.as_deref(),
            Some(&worker_cancelled),
            drain_active_on_drop,
        )
    });
    async move {
        let mut guard = CancelOnDrop {
            flag: cancelled,
            armed: true,
        };
        let result = task
            .await
            .map_err(|error| format!("CORE_TASK_FAILED: {error}"))?;
        guard.armed = false;
        result
    }
}

struct CancelOnDrop {
    flag: Arc<AtomicBool>,
    armed: bool,
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if self.armed {
            self.flag.store(true, Ordering::SeqCst);
        }
    }
}
