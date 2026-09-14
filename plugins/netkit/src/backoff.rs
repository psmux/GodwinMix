//! Reconnect timing.
//!
//! A destination that is down should not be hammered, and a destination that
//! came back should not be waited on for a minute. The shape is the one the
//! core's outputs already use: double from a floor, stop at a ceiling, reset on
//! the first success.

use std::time::Duration;

/// Doubling backoff with a floor and a ceiling.
#[derive(Debug, Clone, Copy)]
pub struct Backoff {
    first_ms: u64,
    max_ms: u64,
    next_ms: u64,
    attempts: u32,
}

impl Backoff {
    /// The default for a network output: half a second, doubling to thirty.
    pub fn new() -> Backoff {
        Backoff::with(500, 30_000)
    }

    pub fn with(first_ms: u64, max_ms: u64) -> Backoff {
        let first_ms = first_ms.max(1);
        Backoff {
            first_ms,
            max_ms: max_ms.max(first_ms),
            next_ms: first_ms,
            attempts: 0,
        }
    }

    /// How long to wait before the next attempt, and move the counter on.
    pub fn take(&mut self) -> Duration {
        let wait = self.next_ms;
        self.attempts += 1;
        self.next_ms = (self.next_ms.saturating_mul(2)).min(self.max_ms);
        Duration::from_millis(wait)
    }

    /// What the next wait would be, without moving the counter.
    pub fn peek(&self) -> Duration {
        Duration::from_millis(self.next_ms)
    }

    /// How many attempts have been made since the last success.
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// It worked. Start again from the floor.
    pub fn reset(&mut self) {
        self.next_ms = self.first_ms;
        self.attempts = 0;
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Backoff::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_doubles_from_the_floor_and_stops_at_the_ceiling() {
        let mut b = Backoff::with(100, 800);
        assert_eq!(b.take(), Duration::from_millis(100));
        assert_eq!(b.take(), Duration::from_millis(200));
        assert_eq!(b.take(), Duration::from_millis(400));
        assert_eq!(b.take(), Duration::from_millis(800));
        assert_eq!(b.take(), Duration::from_millis(800));
        assert_eq!(b.attempts(), 5);
    }

    #[test]
    fn a_success_puts_it_back_to_the_floor() {
        let mut b = Backoff::with(100, 800);
        b.take();
        b.take();
        b.reset();
        assert_eq!(b.peek(), Duration::from_millis(100));
        assert_eq!(b.attempts(), 0);
    }

    #[test]
    fn a_ceiling_below_the_floor_is_raised_to_it() {
        let mut b = Backoff::with(1000, 10);
        assert_eq!(b.take(), Duration::from_millis(1000));
        assert_eq!(b.take(), Duration::from_millis(1000));
    }

    #[test]
    fn peek_does_not_move_the_counter() {
        let b = Backoff::with(250, 4000);
        assert_eq!(b.peek(), Duration::from_millis(250));
        assert_eq!(b.peek(), Duration::from_millis(250));
        assert_eq!(b.attempts(), 0);
    }
}
