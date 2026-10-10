//! A retry that is armed once, and cannot stay armed for ever.
//!
//! A source whose server has gone away posts a burst of errors, and an output
//! whose connection dies posts several. Each would arm its own retry, so the
//! first to arrive claims the right and the rest are refused until the retry
//! has run. That claim used to be a plain flag, cleared by the retry itself,
//! and every path on which the retry did not run left it set: a restart that
//! found an earlier one still inside its turn, a worker thread that could not
//! be started. After that every later retry was refused and the source stayed
//! down until somebody restarted it by hand.
//!
//! So the claim carries the time it was made. One older than the longest wait
//! any retry is scheduled after has outlived the retry it stood for, and the
//! next claim takes it over and says so in the log.

use parking_lot::Mutex;
use std::time::{Duration, Instant};
use tracing::warn;

#[derive(Debug)]
pub struct Armed {
    since: Mutex<Option<Instant>>,
    /// How long a claim may stand before it is taken to be lost.
    stale_after: Duration,
}

impl Armed {
    pub fn new(stale_after: Duration) -> Self {
        Self { since: Mutex::new(None), stale_after }
    }

    /// Claim the one retry. False while a claim younger than `stale_after`
    /// stands.
    pub fn try_arm(&self, id: &str) -> bool {
        let mut since = self.since.lock();
        if let Some(at) = *since {
            let held = at.elapsed();
            if held < self.stale_after {
                return false;
            }
            warn!(
                id,
                held_ms = held.as_millis() as u64,
                "a retry armed for this never ran; arming a new one"
            );
        }
        *since = Some(Instant::now());
        true
    }

    /// The retry has run, or will not: the next one may be armed.
    pub fn disarm(&self) {
        *self.since.lock() = None;
    }

    /// Whether a claim younger than `stale_after` stands.
    pub fn is_armed(&self) -> bool {
        self.since.lock().is_some_and(|at| at.elapsed() < self.stale_after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_claim_at_a_time_until_disarmed() {
        let a = Armed::new(Duration::from_secs(60));
        assert!(a.try_arm("cam"));
        assert!(!a.try_arm("cam"), "a second claim was let in beside the first");
        assert!(a.is_armed());
        a.disarm();
        assert!(!a.is_armed());
        assert!(a.try_arm("cam"));
    }

    /// The wedge: a claim nobody clears. It must give way on its own.
    #[test]
    fn a_claim_that_was_never_cleared_gives_way() {
        let a = Armed::new(Duration::from_millis(30));
        assert!(a.try_arm("cam"));
        assert!(!a.try_arm("cam"));
        std::thread::sleep(Duration::from_millis(40));
        assert!(!a.is_armed(), "a lost claim still reads armed");
        assert!(a.try_arm("cam"), "a lost claim refused the next retry");
        assert!(!a.try_arm("cam"), "and the new claim stands like any other");
    }
}
