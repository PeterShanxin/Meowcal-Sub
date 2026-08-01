use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::Notify;

/// Prevents early WebView IPC from observing in-memory defaults before Tauri setup loads disk.
pub struct StartupGate {
    ready: AtomicBool,
    notify: Notify,
}

impl StartupGate {
    pub fn mark_ready(&self) {
        self.ready.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }

    pub async fn wait_until_ready(&self) -> Result<(), String> {
        if self.ready.load(Ordering::Acquire) {
            return Ok(());
        }

        let notified = self.notify.notified();
        if self.ready.load(Ordering::Acquire) {
            return Ok(());
        }

        tokio::time::timeout(Duration::from_secs(5), notified)
            .await
            .map_err(|_| "Application startup timed out while loading settings".to_string())?;

        self.ready
            .load(Ordering::Acquire)
            .then_some(())
            .ok_or_else(|| "Application settings are not ready".to_string())
    }
}

impl Default for StartupGate {
    fn default() -> Self {
        Self {
            ready: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StartupGate;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn releases_after_persisted_settings_are_loaded() {
        let gate = Arc::new(StartupGate::default());
        let waiting_gate = Arc::clone(&gate);
        let waiter = tokio::spawn(async move { waiting_gate.wait_until_ready().await });

        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(!waiter.is_finished());

        gate.mark_ready();
        assert!(waiter.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn is_immediate_once_ready() {
        let gate = StartupGate::default();
        gate.mark_ready();

        tokio::time::timeout(Duration::from_millis(50), gate.wait_until_ready())
            .await
            .expect("ready gate should not wait")
            .expect("ready gate should succeed");
    }
}
