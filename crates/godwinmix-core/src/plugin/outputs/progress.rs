//! Whether a counter the far end drives is still moving.
//!
//! An SRT or RIST output used to be live once its counters were above zero,
//! and they never go back down: a receiver that vanished behind a pulled
//! cable left an output that said `live` for the rest of the show. What the
//! far end sends back (SRT's acknowledgements, RIST's receiver reports) only
//! keeps coming while it is there, so the output is live while that count
//! moves and for [`QUIET_FOR`] after, and no longer.

use parking_lot::Mutex;
use std::time::{Duration, Instant};

/// SRT acknowledges every 10 ms and RIST reports at least once a second, so
/// six seconds without either is a receiver that is not answering.
pub const QUIET_FOR: Duration = Duration::from_secs(6);

#[derive(Default)]
pub struct Progress {
    seen: Mutex<Option<(u64, Instant)>>,
}

impl Progress {
    /// Forget what was seen, for a freshly built pipeline.
    pub fn reset(&self) {
        *self.seen.lock() = None;
    }

    /// True while `count` is above zero and changed within `QUIET_FOR` of
    /// `now`. A count that went down (a new socket, a new session) is a
    /// change like any other.
    pub fn live(&self, count: u64, now: Instant) -> bool {
        if count == 0 {
            *self.seen.lock() = None;
            return false;
        }
        let mut seen = self.seen.lock();
        match *seen {
            Some((before, at)) if before == count => now.saturating_duration_since(at) < QUIET_FOR,
            _ => {
                *seen = Some((count, now));
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_count_that_stops_moving_stops_being_live_and_a_new_one_starts_again() {
        let p = Progress::default();
        let t = Instant::now();
        assert!(!p.live(0, t), "nothing has come back yet");
        assert!(p.live(10, t));
        assert!(p.live(20, t + Duration::from_secs(5)));
        assert!(p.live(20, t + Duration::from_secs(10)), "inside the quiet window");
        assert!(!p.live(20, t + Duration::from_secs(5) + QUIET_FOR), "a receiver that stopped answering read live");
        assert!(p.live(3, t + Duration::from_secs(60)), "a fresh socket counting from nothing is live again");
        p.reset();
        assert!(p.live(3, t + Duration::from_secs(61)));
    }
}
