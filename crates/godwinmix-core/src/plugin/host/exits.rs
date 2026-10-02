//! How soon a plugin whose process exited by itself is brought back.
//!
//! The first exit is answered at once: a plugin killed by an operator, by the
//! out of memory killer or by a crash that will not happen twice should cost
//! the picture a second, not the twelve the stall timer used to take. A plugin
//! that keeps dying is a different thing, and restarting it as fast as it can
//! die is a loop that eats the machine the programme is running on. So exits
//! that follow each other within [`WINDOW`] count as one streak, and each one
//! in a streak waits longer than the last.
//!
//! A streak is counted by exits, not by frames: a plugin that starts, sends a
//! few frames and dies again would otherwise look healthy between deaths and
//! never be slowed down.

use std::time::{Duration, Instant};

/// Exits closer together than this belong to one streak. Long enough to span
/// several restarts at the longest delay, short enough that a plugin which
/// ran for a while and then died is treated as new.
pub const WINDOW: Duration = Duration::from_secs(30);

/// The longest a streak waits between restarts.
pub const LONGEST: Duration = Duration::from_secs(10);

/// The exits of one instance that came close together.
#[derive(Debug, Default, Clone)]
pub struct Streak {
    count: u32,
    last: Option<Instant>,
}

impl Streak {
    /// Count an exit at `now`. Answers how many exits came before it in this
    /// streak: 0 for the first, which is restarted at once.
    pub fn record(&mut self, now: Instant) -> u32 {
        let close = self.last.is_some_and(|t| now.saturating_duration_since(t) < WINDOW);
        self.count = if close { self.count + 1 } else { 0 };
        self.last = Some(now);
        self.count
    }

    /// The wait after a failure: for an exit, counted and timed as below;
    /// for anything else, `otherwise`, and the streak is left as it was.
    pub fn wait_after(&mut self, exited: bool, otherwise: Duration) -> Duration {
        if !exited {
            return otherwise;
        }
        self.record(Instant::now());
        self.delay()
    }

    /// The wait before restarting after the exit `record` just counted: none
    /// for the first, then one second, doubling, up to [`LONGEST`].
    pub fn delay(&self) -> Duration {
        match self.count {
            0 => Duration::ZERO,
            n => (Duration::from_secs(1) * 2u32.saturating_pow(n - 1)).min(LONGEST),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_exit_is_at_once_and_a_crash_loop_slows_down() {
        let mut streak = Streak::default();
        let t0 = Instant::now();
        let waits: Vec<Duration> = (0..7)
            .map(|i| {
                streak.record(t0 + Duration::from_secs(i * 2));
                streak.delay()
            })
            .collect();
        let secs: Vec<u64> = waits.iter().map(Duration::as_secs).collect();
        assert_eq!(secs, [0, 1, 2, 4, 8, 10, 10]);
    }

    #[test]
    fn an_exit_long_after_the_last_starts_a_new_streak() {
        let mut streak = Streak::default();
        let t0 = Instant::now();
        streak.record(t0);
        assert_eq!(streak.record(t0 + Duration::from_secs(1)), 1);
        assert_eq!(streak.record(t0 + WINDOW + Duration::from_secs(2)), 0);
        assert_eq!(streak.delay(), Duration::ZERO);
    }
}
