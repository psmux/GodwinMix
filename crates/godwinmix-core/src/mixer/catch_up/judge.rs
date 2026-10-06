//! The catch up decision, on numbers alone.

use std::time::{Duration, Instant};

/// The lead every frame must have had, for the whole run, before anything moves.
pub const ABOVE: i64 = 200_000_000;
/// How much lead a caught up source is left with.
pub const KEEP: i64 = 40_000_000;
/// Consecutive supervisor ticks over [`ABOVE`] before a catch up: two seconds.
pub const TICKS_TO_ACT: u32 = 4;
/// A second catch up this soon after the first means the source is not live.
pub const RELAPSE: Duration = Duration::from_secs(10);

/// What the guard decided on one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Hold,
    /// Move the source this many nanoseconds earlier.
    CatchUp(i64),
    /// The source ran ahead again after a catch up; it is left alone from now.
    GiveUp,
}

/// The decision, kept apart from GStreamer so it can be tested on numbers.
#[derive(Debug, Default)]
pub struct Judge {
    over: u32,
    least: i64,
    /// When the last catch up was, and the lead it removed.
    last: Option<(Instant, i64)>,
    off: bool,
}

impl Judge {
    /// One supervisor tick, with the least lead seen since the last one, or
    /// `None` when no frame arrived.
    pub fn tick(&mut self, least: Option<i64>, now: Instant) -> Verdict {
        if self.off {
            return Verdict::Hold;
        }
        let Some(least) = least.filter(|l| *l > ABOVE) else {
            self.over = 0;
            return Verdict::Hold;
        };
        self.least = if self.over == 0 { least } else { self.least.min(least) };
        self.over += 1;
        if self.over < TICKS_TO_ACT {
            return Verdict::Hold;
        }
        self.over = 0;
        let again = self.last.is_some_and(|(t, before)| {
            now.duration_since(t) < RELAPSE && self.least * 4 >= before * 3
        });
        if again {
            self.off = true;
            return Verdict::GiveUp;
        }
        self.last = Some((now, self.least));
        Verdict::CatchUp(self.least - KEEP)
    }

    /// A restarted or seeked source starts from nothing.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

