use super::{call, CoreStatus, TRANSLATION};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long a translation session waits before restarting a failed engine,
/// by consecutive failure; the last wait repeats. A machine too busy for the
/// readiness check can recover without the user, and a broken engine is not
/// restarted in a loop.
const RETRY_DELAYS: [Duration; 4] = [
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(120),
    Duration::from_secs(300),
];

#[derive(Clone, Copy)]
struct Failures {
    last: Option<Instant>,
    consecutive: usize,
}

struct Flight {
    cancelled: Arc<AtomicBool>,
    inference: bool,
}

struct RecoveryState {
    active: Mutex<Option<Flight>>,
    stopped: AtomicBool,
    failed: AtomicBool,
    failures: Mutex<Failures>,
    cpu_locked: AtomicBool,
}
impl RecoveryState {
    const fn new() -> Self {
        Self {
            active: Mutex::new(None),
            stopped: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            failures: Mutex::new(Failures {
                last: None,
                consecutive: 0,
            }),
            cpu_locked: AtomicBool::new(false),
        }
    }
    fn begin(&self, inference: bool) -> Option<Arc<AtomicBool>> {
        let mut active = self.active.lock().ok()?;
        if active.is_some() || self.stopped.load(Ordering::SeqCst) {
            return None;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        *active = Some(Flight {
            cancelled: cancelled.clone(),
            inference,
        });
        Some(cancelled)
    }
    fn cancel(&self) {
        if let Ok(active) = self.active.lock() {
            self.stopped.store(true, Ordering::SeqCst);
            if let Some(flight) = active.as_ref() {
                if flight.inference {
                    self.cpu_locked.store(true, Ordering::SeqCst);
                }
                flight.cancelled.store(true, Ordering::SeqCst);
            }
        }
    }
    fn record(&self, ready: bool, now: Instant) {
        self.failed.store(!ready, Ordering::SeqCst);
        if let Ok(mut failures) = self.failures.lock() {
            *failures = if ready {
                Failures {
                    last: None,
                    consecutive: 0,
                }
            } else {
                Failures {
                    last: Some(now),
                    consecutive: failures.consecutive + 1,
                }
            };
        }
    }
    fn retry_due(&self, now: Instant) -> bool {
        let Ok(failures) = self.failures.lock() else {
            return false;
        };
        let wait = RETRY_DELAYS[failures.consecutive.clamp(1, RETRY_DELAYS.len()) - 1];
        failures
            .last
            .is_none_or(|last| now.saturating_duration_since(last) >= wait)
    }
    fn finish(&self, cancelled: &Arc<AtomicBool>) {
        if let Ok(mut active) = self.active.lock() {
            if active
                .as_ref()
                .is_some_and(|flight| Arc::ptr_eq(&flight.cancelled, cancelled))
            {
                *active = None;
            }
        }
    }
}
static STATE: RecoveryState = RecoveryState::new();

pub(super) fn cpu_locked() -> bool {
    STATE.cpu_locked.load(Ordering::SeqCst)
}
pub(super) fn lock_cpu() {
    STATE.cpu_locked.store(true, Ordering::SeqCst);
}
pub fn recovering() -> bool {
    STATE.active.lock().is_ok_and(|active| active.is_some())
}
pub fn recovery_failed() -> bool {
    STATE.failed.load(Ordering::SeqCst)
}
pub(super) fn clear_failure() {
    STATE.record(true, Instant::now());
    STATE.stopped.store(false, Ordering::SeqCst);
}
pub(super) fn cancel() {
    STATE.cancel();
}

pub fn recover_inference(receipt: String, reason: &'static str) {
    start(
        "recoverInference",
        json!({"receipt":receipt,"reason":reason}),
    );
}

/// Start a stopped engine again; a failed one only once its wait has passed.
pub fn recover_transport() {
    if !recovery_failed() || STATE.retry_due(Instant::now()) {
        start("ready", json!({}));
    }
}

fn start(method: &'static str, params: Value) {
    let Some(cancelled) = STATE.begin(method == "recoverInference") else {
        return;
    };
    super::invalidate_readiness();
    tokio::task::spawn_blocking(move || {
        let result: Result<CoreStatus, String> = call(
            &TRANSLATION,
            method,
            params,
            Duration::from_secs(120),
            None,
            Some(&cancelled),
            false,
        );
        if !cancelled.load(Ordering::SeqCst) {
            match result {
                Ok(status) => STATE.record(status.ready, Instant::now()),
                Err(error) => {
                    // An interrupted correctness check cannot certify another GPU
                    // start. Keep the app-lifetime CPU policy across Core replacement.
                    if method == "recoverInference" {
                        lock_cpu();
                    }
                    STATE.record(false, Instant::now());
                    super::invalidate_readiness();
                    tracing::warn!(%error, "Translation engine recovery failed");
                }
            }
        }
        STATE.finish(&cancelled);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlapping_failures_admit_one_recovery_and_cancel_keeps_cpu_policy() {
        let state = Arc::new(RecoveryState::new());
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let state = state.clone();
                std::thread::spawn(move || state.begin(true))
            })
            .collect();
        let admitted: Vec<_> = workers
            .into_iter()
            .filter_map(|worker| worker.join().unwrap())
            .collect();
        assert_eq!(admitted.len(), 1);
        state.cancel();
        assert!(admitted[0].load(Ordering::SeqCst));
        assert!(state.cpu_locked.load(Ordering::SeqCst));
        assert!(state.begin(true).is_none());
        state.finish(&admitted[0]);
        assert!(
            state.begin(false).is_none(),
            "shutdown blocks late recovery work"
        );
        state.stopped.store(false, Ordering::SeqCst);
        let next = state.begin(false).unwrap();
        state.finish(&admitted[0]);
        assert!(
            state.begin(false).is_none(),
            "late worker cannot clear a new flight"
        );
        assert!(state.cpu_locked.load(Ordering::SeqCst));
        state.finish(&next);
    }
    #[test]
    fn a_failed_engine_is_retried_after_a_growing_wait() {
        let state = RecoveryState::new();
        let start = Instant::now();
        assert!(state.retry_due(start), "nothing has failed yet");
        let mut failed_at = start;
        for wait in [30, 60, 120, 300, 300] {
            state.record(false, failed_at);
            let due = failed_at + Duration::from_secs(wait);
            assert!(!state.retry_due(due - Duration::from_secs(1)), "{wait}");
            assert!(state.retry_due(due), "{wait}");
            failed_at = due;
        }
        state.record(true, failed_at);
        assert!(!state.failed.load(Ordering::SeqCst));
        state.record(false, failed_at);
        assert!(!state.retry_due(failed_at + Duration::from_secs(29)));
        assert!(state.retry_due(failed_at + Duration::from_secs(30)));
    }
    #[test]
    fn a_transport_timeout_does_not_lock_cpu() {
        let state = RecoveryState::new();
        let flight = state.begin(false).unwrap();
        state.cancel();
        state.finish(&flight);
        assert!(!state.cpu_locked.load(Ordering::SeqCst));
    }
}
