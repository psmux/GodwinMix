//! How long an output may stay down with nothing on the way to bring it back.
//!
//! An output comes back by one of two roads: its pipeline posts an error and
//! the mixer arms a reconnect with the policy's backoff, or its feed queue
//! fills and the overflow watchdog rebuilds it. A rebuilt pipeline that does
//! neither, because its sink never posted an error and never took enough to
//! fill the queue, used to stay down for good. A soak test on Windows had the
//! programme output to a local RTMP server sit in `reconnecting` for eight
//! minutes after a forced reconnect, with no error and no further attempt,
//! until the show was killed and started again.
//!
//! So the time an output went down is kept, from its last build or from the
//! moment it stopped being live, whichever came later, and the mixer's tick
//! rebuilds one that has been down for [`DOWN_FOR`] with no reconnect armed
//! or running. A build resets the clock, so a far end that never answers is
//! asked again once every [`DOWN_FOR`] and no more often.

use parking_lot::Mutex;
use std::time::{Duration, Instant};

/// Long enough for any real connect, a TLS handshake to a platform on a slow
/// uplink included, and for the backoff of an error path to have run.
pub const DOWN_FOR: Duration = Duration::from_secs(20);

#[derive(Default)]
pub struct Deadline {
    down_since: Mutex<Option<Instant>>,
}

impl Deadline {
    /// A pipeline was just built: the clock starts again from now.
    pub fn built(&self, now: Instant) {
        *self.down_since.lock() = Some(now);
    }

    /// The output's live state, as the tick reads it.
    pub fn live(&self, live: bool, now: Instant) {
        let mut since = self.down_since.lock();
        match (live, *since) {
            (true, _) => *since = None,
            (false, None) => *since = Some(now),
            (false, Some(_)) => {}
        }
    }

    /// Down for longer than `DOWN_FOR` at `now`.
    pub fn overdue(&self, now: Instant) -> bool {
        self.down_since.lock().is_some_and(|t| now.saturating_duration_since(t) >= DOWN_FOR)
    }

    /// How long it has been down, for the log line.
    pub fn down_for(&self, now: Instant) -> Option<Duration> {
        self.down_since.lock().map(|t| now.saturating_duration_since(t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_that_never_comes_live_is_overdue_after_the_limit_and_not_before() {
        let d = Deadline::default();
        let t = Instant::now();
        d.built(t);
        d.live(false, t + Duration::from_secs(5));
        assert!(!d.overdue(t + DOWN_FOR - Duration::from_millis(1)));
        assert!(d.overdue(t + DOWN_FOR));
    }

    #[test]
    fn coming_live_clears_it_and_going_down_starts_it_from_then() {
        let d = Deadline::default();
        let t = Instant::now();
        d.built(t);
        d.live(true, t + Duration::from_secs(1));
        assert!(!d.overdue(t + Duration::from_secs(600)), "live is never overdue");
        let dropped = t + Duration::from_secs(300);
        d.live(false, dropped);
        d.live(false, dropped + Duration::from_secs(10));
        assert!(!d.overdue(dropped + DOWN_FOR - Duration::from_millis(1)), "a hiccup is not a stuck output");
        assert!(d.overdue(dropped + DOWN_FOR));
    }

    #[test]
    fn a_new_build_starts_the_clock_again() {
        let d = Deadline::default();
        let t = Instant::now();
        d.built(t);
        assert!(d.overdue(t + DOWN_FOR));
        d.built(t + DOWN_FOR);
        assert!(!d.overdue(t + DOWN_FOR + Duration::from_secs(1)));
    }
}
