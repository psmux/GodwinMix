//! A count of automatic moves, which stops sync when they come too fast.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// At most `limit` automatic moves in any `window`. One more and sync pauses
/// for `pause`, which a person sees in the log and in `list`.
#[derive(Debug)]
pub struct Guard {
    pub window: Duration,
    pub limit: usize,
    pub pause: Duration,
    moves: VecDeque<Instant>,
    paused_until: Option<Instant>,
}

impl Default for Guard {
    fn default() -> Guard {
        Guard { window: Duration::from_secs(1), limit: 6, pause: Duration::from_secs(3), moves: VecDeque::new(), paused_until: None }
    }
}

/// What the guard says about one more move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Go,
    /// This move tipped it over: say so once.
    Tripped,
    Paused,
}

impl Guard {
    pub fn check(&mut self, now: Instant) -> Verdict {
        if let Some(until) = self.paused_until {
            if now < until {
                return Verdict::Paused;
            }
            self.paused_until = None;
            self.moves.clear();
        }
        while self.moves.front().is_some_and(|t| now.duration_since(*t) >= self.window) {
            self.moves.pop_front();
        }
        if self.moves.len() >= self.limit {
            self.paused_until = Some(now + self.pause);
            return Verdict::Tripped;
        }
        self.moves.push_back(now);
        Verdict::Go
    }

    pub fn paused(&self, now: Instant) -> bool {
        self.paused_until.is_some_and(|until| now < until)
    }
}

/// The sentence for a tripped guard.
pub const TRIPPED: &str = "follow and lead moved the programme more than six times in a second, \
    which is a loop (two switchers or two plugins chasing each other), so sync is paused for three \
    seconds. Check that no source is mapped on a leading switcher and a following one that feed \
    each other, and that only one program drives GodwinMix from this switcher";
