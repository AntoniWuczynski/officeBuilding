//! Lock helper shared by the newer modules.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Lock a mutex whose data is plain, so a poisoned lock is still good.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
