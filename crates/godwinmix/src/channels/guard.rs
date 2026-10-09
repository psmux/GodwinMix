//! The channels' locks, and the two rules about them: none is taken twice on
//! one thread, and none is held across a call to the mixer or a plugin.
//!
//! A parking_lot mutex taken again by the thread that holds it waits for
//! ever and says nothing. On 2026-10-09 `channel.thumbnail` for a channel id
//! that was not there did that: the refusal was built inside the statement
//! that held `records`, and building it took `records` again. The blocking
//! thread never came back, and every `channel.list` after it queued behind
//! that lock until the core was restarted, with no line in the log.
//!
//! In a debug build each lock writes itself into a list kept per thread while
//! it is held. Taking one this thread already holds panics with its name
//! instead of hanging, and [`assert_free`], said before a call that can
//! block, panics when any is held. A release build keeps none of it.

use parking_lot::{Mutex, MutexGuard};
use std::ops::{Deref, DerefMut};

#[cfg(debug_assertions)]
thread_local! {
    static HELD: std::cell::RefCell<Vec<(usize, &'static str)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// A mutex with a name, for the checks above.
pub struct Lock<T> {
    name: &'static str,
    inner: Mutex<T>,
}

pub struct Guard<'a, T> {
    inner: MutexGuard<'a, T>,
    #[cfg(debug_assertions)]
    key: usize,
}

impl<T> Lock<T> {
    pub fn new(name: &'static str, value: T) -> Self {
        Lock { name, inner: Mutex::new(value) }
    }

    pub fn lock(&self) -> Guard<'_, T> {
        #[cfg(debug_assertions)]
        let key = self.enter();
        Guard {
            inner: self.inner.lock(),
            #[cfg(debug_assertions)]
            key,
        }
    }

    /// Note this lock as held here, and refuse to take it a second time.
    #[cfg(debug_assertions)]
    fn enter(&self) -> usize {
        let key = &self.inner as *const Mutex<T> as usize;
        HELD.with(|held| {
            let mut held = held.borrow_mut();
            assert!(
                !held.iter().any(|(k, _)| *k == key),
                "the channels' `{}` lock was taken again by the thread that holds it, which would wait for ever",
                self.name
            );
            held.push((key, self.name));
        });
        key
    }
}

impl<T> Deref for Guard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T> DerefMut for Guard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

#[cfg(debug_assertions)]
impl<T> Drop for Guard<'_, T> {
    fn drop(&mut self) {
        let key = self.key;
        HELD.with(|held| {
            let mut held = held.borrow_mut();
            if let Some(at) = held.iter().rposition(|(k, _)| *k == key) {
                held.remove(at);
            }
        });
    }
}

/// Said before a call that can block: the mixer's queue, a plugin. Panics in
/// a debug build when this thread holds any of the channels' locks.
pub fn assert_free(calling: &str) {
    #[cfg(debug_assertions)]
    HELD.with(|held| {
        let held = held.borrow();
        assert!(
            held.is_empty(),
            "calling {calling} while holding the channels' {:?} lock; a slow answer would hold every channel call behind it",
            held.iter().map(|(_, name)| *name).collect::<Vec<_>>()
        );
    });
    #[cfg(not(debug_assertions))]
    let _ = calling;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(debug_assertions)]
    fn a_lock_taken_twice_on_one_thread_panics_instead_of_waiting() {
        let lock = Lock::new("records", 1);
        let first = lock.lock();
        let again = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(lock.lock())));
        assert!(again.is_err());
        drop(first);
        // Let go, it can be taken again, and nothing is left noted as held.
        assert_eq!(*lock.lock(), 1);
        assert_free("a test");
    }

    #[test]
    #[cfg(debug_assertions)]
    fn a_call_out_under_a_lock_panics() {
        let lock = Lock::new("live", ());
        let _held = lock.lock();
        let out = std::panic::catch_unwind(|| assert_free("the mixer"));
        assert!(out.is_err());
    }
}
