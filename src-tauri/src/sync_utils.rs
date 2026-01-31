//! Synchronization utilities for safe mutex/RwLock handling.
//!
//! This module provides helper functions that recover from poisoned locks
//! instead of panicking. Mutex poisoning occurs when a thread panics while
//! holding a lock - these helpers ensure the application can continue
//! operating even if that happens.

use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};
use tracing::warn;

/// Acquires a mutex lock, recovering if the mutex was poisoned.
///
/// If the mutex is poisoned (a thread panicked while holding it),
/// this function logs a warning and returns the inner value anyway.
/// This prevents cascading failures where one panic brings down the
/// entire application.
///
/// # Example
/// ```ignore
/// use crate::sync_utils::lock_or_recover;
///
/// let mutex = Mutex::new(42);
/// let guard = lock_or_recover(&mutex);
/// assert_eq!(*guard, 42);
/// ```
pub fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| {
        warn!("Mutex poisoned, recovering - a thread previously panicked while holding this lock");
        poisoned.into_inner()
    })
}

/// Acquires a read lock on an RwLock, recovering if poisoned.
///
/// Similar to [`lock_or_recover`], but for RwLock read access.
pub fn read_or_recover<T>(rwlock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    rwlock.read().unwrap_or_else(|poisoned| {
        warn!(
            "RwLock poisoned (read), recovering - a thread previously panicked while holding this lock"
        );
        poisoned.into_inner()
    })
}

/// Acquires a write lock on an RwLock, recovering if poisoned.
///
/// Similar to [`lock_or_recover`], but for RwLock write access.
pub fn write_or_recover<T>(rwlock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    rwlock.write().unwrap_or_else(|poisoned| {
        warn!(
            "RwLock poisoned (write), recovering - a thread previously panicked while holding this lock"
        );
        poisoned.into_inner()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_lock_or_recover_normal() {
        let mutex = Mutex::new(42);
        let guard = lock_or_recover(&mutex);
        assert_eq!(*guard, 42);
    }

    #[test]
    fn test_lock_or_recover_after_poison() {
        let mutex = Arc::new(Mutex::new(42));
        let mutex_clone = Arc::clone(&mutex);

        // Spawn a thread that panics while holding the lock
        let handle = thread::spawn(move || {
            let _guard = mutex_clone.lock().unwrap();
            panic!("intentional panic to poison mutex");
        });

        // Wait for the thread to finish (it will panic)
        let _ = handle.join();

        // The mutex should now be poisoned, but lock_or_recover should still work
        let guard = lock_or_recover(&mutex);
        assert_eq!(*guard, 42);
    }

    #[test]
    fn test_read_or_recover_normal() {
        let rwlock = RwLock::new("test");
        let guard = read_or_recover(&rwlock);
        assert_eq!(*guard, "test");
    }

    #[test]
    fn test_write_or_recover_normal() {
        let rwlock = RwLock::new(0);
        let mut guard = write_or_recover(&rwlock);
        *guard = 100;
        assert_eq!(*guard, 100);
    }
}
