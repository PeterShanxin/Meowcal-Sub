use super::CoreStatus;
use std::sync::Mutex;

#[derive(Default)]
pub(super) struct Readiness {
    pub status: Option<CoreStatus>,
    generation: u64,
    installs: usize,
}

pub(super) static CACHE: Mutex<Readiness> = Mutex::new(Readiness {
    status: None,
    generation: 0,
    installs: 0,
});

impl Readiness {
    fn invalidate(&mut self) {
        self.generation += 1;
        if let Some(status) = self.status.as_mut() {
            status.ready = false;
        }
    }

    fn record(&mut self, status: CoreStatus, generation: u64) -> bool {
        if generation != self.generation || self.installs > 0 {
            return false;
        }
        self.status = Some(status);
        true
    }
}

pub(super) fn cached_status() -> Option<CoreStatus> {
    CACHE.lock().ok().and_then(|cache| cache.status.clone())
}

pub(super) fn invalidate() {
    if let Ok(mut cache) = CACHE.lock() {
        cache.invalidate();
    }
}

pub(super) fn generation() -> Result<u64, String> {
    CACHE
        .lock()
        .map(|cache| cache.generation)
        .map_err(|_| "CORE_READINESS_LOCK_POISONED".to_string())
}

pub(super) fn installing() -> bool {
    CACHE.lock().map(|cache| cache.installs > 0).unwrap_or(true)
}

pub(super) fn record(status: CoreStatus, generation: u64) -> bool {
    CACHE
        .lock()
        .map(|mut cache| cache.record(status, generation))
        .unwrap_or(false)
}

// The blocking worker owns this guard, including when its async caller is dropped.
pub(super) struct Install;

impl Install {
    pub(super) fn begin() -> Result<Self, String> {
        let mut cache = CACHE
            .lock()
            .map_err(|_| "CORE_READINESS_LOCK_POISONED".to_string())?;
        cache.installs += 1;
        cache.invalidate();
        Ok(Self)
    }
}

impl Drop for Install {
    fn drop(&mut self) {
        if let Ok(mut cache) = CACHE.lock() {
            cache.installs -= 1;
            cache.invalidate();
        }
    }
}
