//! How long an output has been live without a break.
//!
//! A page opened on a mixer that was left streaming has to say "YouTube for
//! 1:56:23", and the page cannot know that from its own clock: it was not
//! open when the stream started. So the output counts it, from the moment it
//! was last seen live, and `live_secs` in its status carries the answer. A
//! drop resets it, because "live since" after a reconnect is when it came
//! back.

use parking_lot::Mutex;
use std::time::Instant;

#[derive(Debug, Default)]
pub struct LiveSince(Mutex<Option<Instant>>);

impl LiveSince {
    /// What the last liveness check found.
    pub fn note(&self, live: bool, now: Instant) {
        let mut since = self.0.lock();
        match (live, *since) {
            (true, None) => *since = Some(now),
            (false, Some(_)) => *since = None,
            _ => {}
        }
    }

    /// Whole seconds live, while it is.
    pub fn secs(&self, now: Instant) -> Option<u64> {
        self.0.lock().map(|at| now.saturating_duration_since(at).as_secs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn counts_from_the_first_live_check_and_starts_again_after_a_drop() {
        let since = LiveSince::default();
        let t0 = Instant::now();
        assert_eq!(since.secs(t0), None);
        since.note(true, t0);
        since.note(true, t0 + Duration::from_secs(5));
        assert_eq!(since.secs(t0 + Duration::from_secs(90)), Some(90));
        since.note(false, t0 + Duration::from_secs(91));
        assert_eq!(since.secs(t0 + Duration::from_secs(92)), None);
        since.note(true, t0 + Duration::from_secs(100));
        assert_eq!(since.secs(t0 + Duration::from_secs(101)), Some(1));
    }
}
