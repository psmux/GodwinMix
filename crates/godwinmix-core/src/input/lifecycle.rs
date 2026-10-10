//! Who may restart or stop a source's pipeline, and when.
//!
//! A restart and a stop both take a pipeline to NULL, and both can take a long
//! time: a streaming thread parked in a queue nobody reads, a plugin process
//! that is slow to die, a device that has to be let go. So neither runs on the
//! mixer thread any more (see `mixer::offload`), which means two of them can be
//! in flight for one source at once. This is what keeps them in order:
//!
//! * One restart at a time. The mixer claims the restart before it hands the
//!   work to a thread, and a second claim while the first is running fails, so
//!   the burst of restarts a dead source asks for collapses into one.
//! * A stop wins. It marks the source stopped before it waits its turn, and a
//!   restart that gets the lock after that finds the mark and does nothing,
//!   so a source removed in the middle of a restart does not come back.
//! * Every wait for the other one is bounded and says so in the log.
//!
//! An output's reconnect and detach keep the same order with the same type:
//! a reconnect is its restart, a detach its stop.

use parking_lot::{Mutex, MutexGuard};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tracing::warn;

/// How long a restart or a stop waits for the other one to finish. Longer
/// than any restart that is working, short enough that a stuck one is named in
/// the log well before anybody goes looking.
pub const LIFECYCLE_WAIT: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct Lifecycle {
    lock: Mutex<()>,
    restarting: AtomicBool,
    stopped: AtomicBool,
    /// When the running restart was claimed, so the mixer can see one that
    /// has hung and stop waiting for it (`mixer::supervise`).
    claimed_at: Mutex<Option<Instant>>,
}

impl Lifecycle {
    /// Claim the one restart this source may have running. False when one
    /// is already running or the source has been stopped.
    pub fn claim_restart(&self) -> bool {
        let claimed = !self.stopped()
            && self
                .restarting
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok();
        if claimed {
            *self.claimed_at.lock() = Some(Instant::now());
        }
        claimed
    }

    /// The restart is over, whichever way it went.
    pub fn end_restart(&self) {
        *self.claimed_at.lock() = None;
        self.restarting.store(false, Ordering::SeqCst);
    }

    /// How long the running restart has been going, if one is.
    pub fn restart_running_for(&self) -> Option<Duration> {
        self.claimed_at.lock().map(|at| at.elapsed())
    }

    /// True while a restart is running.
    pub fn restarting(&self) -> bool {
        self.restarting.load(Ordering::SeqCst)
    }

    /// Say the source is going, before waiting for anything.
    pub fn mark_stopped(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }

    pub fn stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    /// Wait for the other restart or stop of this source to finish, for at
    /// most `LIFECYCLE_WAIT`. None when it did not, with a line saying which
    /// source and what was waiting.
    pub fn enter(&self, id: &str, what: &'static str) -> Option<MutexGuard<'_, ()>> {
        self.enter_within(id, what, LIFECYCLE_WAIT)
    }

    /// The same with a wait of the caller's choosing, for a caller on the
    /// mixer thread that can afford far less than `LIFECYCLE_WAIT`.
    pub fn enter_within(
        &self,
        id: &str,
        what: &'static str,
        wait: Duration,
    ) -> Option<MutexGuard<'_, ()>> {
        let guard = self.lock.try_lock_for(wait);
        if guard.is_none() {
            warn!(
                id,
                what,
                waited_ms = wait.as_millis() as u64,
                "an earlier restart or stop of this has not finished"
            );
        }
        guard
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_restart_at_a_time_and_none_after_a_stop() {
        let l = Lifecycle::default();
        assert!(l.claim_restart());
        assert!(!l.claim_restart(), "a second restart was let in beside the first");
        assert!(l.restart_running_for().is_some(), "a running restart has no start time");
        l.end_restart();
        assert_eq!(l.restart_running_for(), None);
        assert!(l.claim_restart());
        l.end_restart();
        l.mark_stopped();
        assert!(!l.claim_restart(), "a stopped source was let restart");
    }

    #[test]
    fn a_wait_for_a_stuck_holder_ends() {
        let l = Lifecycle::default();
        let _held = l.enter("cam", "restart").expect("the first entry");
        let started = std::time::Instant::now();
        assert!(l.enter("cam", "stop").is_none());
        assert!(started.elapsed() < LIFECYCLE_WAIT * 2);
    }
}
