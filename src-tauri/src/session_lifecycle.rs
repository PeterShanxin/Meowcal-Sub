use crate::sync_utils::lock_or_recover;
use std::future::Future;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::{AbortHandle, JoinHandle};

#[derive(Clone)]
struct CaptureTask {
    abort: AbortHandle,
    done: watch::Receiver<bool>,
}

/// Serializes session transitions through capture cleanup and overlay hiding.
#[derive(Default)]
pub struct SessionLifecycle {
    pub control: tokio::sync::Mutex<()>,
    task: Mutex<Option<CaptureTask>>,
}

impl SessionLifecycle {
    pub fn monitor(&self, task: JoinHandle<()>) {
        let (finished, done) = watch::channel(false);
        *lock_or_recover(&self.task) = Some(CaptureTask {
            abort: task.abort_handle(),
            done,
        });
        tokio::spawn(async move {
            if let Err(error) = task.await {
                if error.is_panic() {
                    tracing::error!(%error, "Translation loop panicked; capture cleanup completed");
                } else {
                    tracing::info!("Translation loop was cancelled");
                }
            }
            finished.send_replace(true);
        });
    }

    pub async fn wait_stopped(&self) -> Result<(), String> {
        // Keep ownership in the slot even if the Stop command itself is dropped.
        let task = lock_or_recover(&self.task).clone();
        if let Some(mut task) = task {
            let completed = match tokio::time::timeout(Duration::from_secs(1), async {
                task.done.wait_for(|done| *done).await.map(|_| ())
            })
            .await
            {
                Ok(result) => result,
                Err(_) => {
                    task.abort.abort();
                    tokio::time::timeout(Duration::from_secs(1), task.done.wait_for(|done| *done))
                        .await
                        .map_err(|_| "Capture is still stopping. Please retry Stop.".to_string())?
                        .map(|_| ())
                }
            };
            completed.map_err(|_| "Capture task monitor disconnected.".to_string())?;
            *lock_or_recover(&self.task) = None;
        }
        Ok(())
    }
}

/// Dropping a pending OCR future cancels its Core transport without touching inference.
pub(crate) async fn until_stopped<F: Future>(
    work: F,
    stop: &mut watch::Receiver<bool>,
) -> Option<F::Output> {
    if *stop.borrow() {
        return None;
    }
    tokio::select! {
        biased;
        _ = stop.changed() => None,
        result = work => Some(result),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    #[tokio::test]
    async fn stop_waits_for_delayed_ocr_to_drop_and_old_cleanup_to_finish() {
        let lifecycle = SessionLifecycle::default();
        let cleaned = Arc::new(AtomicBool::new(false));
        let released = cleaned.clone();
        let (stop, mut receiver) = watch::channel(false);
        let (entered, waiting) = tokio::sync::oneshot::channel();
        lifecycle.monitor(tokio::spawn(async move {
            let _cleanup = scopeguard::guard((), |_| released.store(true, Ordering::SeqCst));
            let _ = entered.send(());
            let result = until_stopped(std::future::pending::<()>(), &mut receiver).await;
            assert!(result.is_none());
            tokio::task::yield_now().await;
        }));
        waiting.await.unwrap();
        stop.send(true).unwrap();
        lifecycle.wait_stopped().await.unwrap();
        assert!(cleaned.load(Ordering::SeqCst));
        assert!(lock_or_recover(&lifecycle.task).is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn bounded_teardown_aborts_a_task_that_does_not_observe_stop() {
        let lifecycle = SessionLifecycle::default();
        let cleaned = Arc::new(AtomicBool::new(false));
        let released = cleaned.clone();
        lifecycle.monitor(tokio::spawn(async move {
            let _cleanup = scopeguard::guard((), |_| released.store(true, Ordering::SeqCst));
            std::future::pending::<()>().await;
        }));
        tokio::task::yield_now().await;
        lifecycle.wait_stopped().await.unwrap();
        assert!(cleaned.load(Ordering::SeqCst));
        lifecycle.wait_stopped().await.unwrap();
    }

    #[tokio::test]
    async fn a_completed_or_already_stopped_ocr_cannot_publish_late_results() {
        let (stop, mut receiver) = watch::channel(false);
        assert_eq!(until_stopped(async { 7 }, &mut receiver).await, Some(7));
        stop.send(true).unwrap();
        assert_eq!(until_stopped(async { 8 }, &mut receiver).await, None);
    }
}
