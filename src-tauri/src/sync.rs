//! Poison-tolerant lock helpers.
//!
//! A poisoned `Mutex`/`RwLock` means a previous holder panicked while holding
//! it. The usual `.unwrap()` then panics for *every* later caller — and because
//! the monitoring loop runs inside a spawned task, one panic there killed the
//! poll loop permanently and silently: the widget froze on its last frame with
//! no error anywhere.
//!
//! Everything behind these locks is a cache, a counter or a config snapshot, so
//! taking the (possibly partially updated) value is strictly better than dying.

use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Lock a mutex, recovering the guard if a previous holder panicked.
pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Take a read guard, recovering it if a previous holder panicked.
pub(crate) fn read<T>(m: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    m.read().unwrap_or_else(|e| e.into_inner())
}

/// Take a write guard, recovering it if a previous holder panicked.
pub(crate) fn write<T>(m: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    m.write().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// A mutex poisoned by a panicking holder must stay usable: that is the
    /// whole point of these helpers.
    #[test]
    fn poisoned_mutex_is_still_usable() {
        let m = Arc::new(Mutex::new(1u32));
        let clone = Arc::clone(&m);
        let _ = std::thread::spawn(move || {
            let _guard = clone.lock().unwrap();
            panic!("holder panicked");
        })
        .join();

        assert!(m.lock().is_err(), "the mutex must actually be poisoned");
        // The raw lock fails, the helper recovers.
        assert_eq!(*lock(&m), 1);
        *lock(&m) = 7;
        assert_eq!(*lock(&m), 7);
    }

    #[test]
    fn poisoned_rwlock_is_still_usable() {
        let m = Arc::new(RwLock::new(String::from("a")));
        let clone = Arc::clone(&m);
        let _ = std::thread::spawn(move || {
            let _guard = clone.write().unwrap();
            panic!("holder panicked");
        })
        .join();

        assert_eq!(&*read(&m), "a");
        write(&m).push('b');
        assert_eq!(&*read(&m), "ab");
    }
}
