//! The last hundred takes, with who made them.
//!
//! `program.history` reads it and `program.revert` uses it to find the shot
//! before this one. Kept in the control plane rather than in the mixer,
//! because "who asked" is a property of the call and the mixer thread has
//! never heard of a token.
//!
//! In memory and bounded. The durable record is the session log, which is
//! append only and which no method exposed to any client can edit.

use crate::api::requests::TakeRecord;
use parking_lot::Mutex;
use std::time::{Duration, Instant};

/// Enough to see a whole show's worth of decisions without holding anything
/// an operator would call a recording.
pub const KEEP: usize = 100;

/// How long a client's claim on the next take stays good for. A take lands on
/// the pipeline clock within a frame or two of the call; two seconds is
/// generous and still short enough that a take nobody asked for is not
/// credited to whoever called last.
const ATTRIBUTION_WINDOW: Duration = Duration::from_secs(2);

#[derive(Debug, Default)]
pub struct History {
    takes: Mutex<Vec<TakeRecord>>,
    /// Who asked for the take that is about to land. The mixer publishes the
    /// event, and the event has never heard of a token, so the caller leaves
    /// its name here on the way past.
    pending: Mutex<Option<(String, Instant)>>,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// Write down a take. Oldest first in the list, newest last.
    pub fn record(&self, source: Option<String>, at_running_time_ms: u64, by: &str, seq: u64) {
        let mut takes = self.takes.lock();
        // The mixer publishes `took` for a take it made itself as well as for
        // one a client asked for, so the same cut can arrive twice. One entry
        // per sequence number keeps the list honest.
        if takes.last().is_some_and(|t| t.seq == seq) {
            return;
        }
        takes.push(TakeRecord { source, at_running_time_ms, by: by.to_string(), seq });
        let len = takes.len();
        if len > KEEP {
            takes.drain(0..len - KEEP);
        }
    }

    /// Say who asked for the take that is about to land.
    pub fn expect(&self, by: &str) {
        *self.pending.lock() = Some((by.to_string(), Instant::now()));
    }

    /// Write down a take that arrived as an event, crediting whoever claimed
    /// it. A take the mixer made itself, rejoining after an ad break, belongs
    /// to "core" and says so.
    pub fn record_event(&self, source: Option<String>, at_running_time_ms: u64, seq: u64) {
        let by = self
            .pending
            .lock()
            .take()
            .filter(|(_, at)| at.elapsed() < ATTRIBUTION_WINDOW)
            .map_or_else(|| "core".to_string(), |(who, _)| who);
        self.record(source, at_running_time_ms, &by, seq);
    }

    /// Newest first, which is the order a person reads a log in.
    pub fn recent(&self, limit: usize) -> Vec<TakeRecord> {
        let takes = self.takes.lock();
        takes.iter().rev().take(limit.clamp(1, KEEP)).cloned().collect()
    }

    /// What `program.revert` takes back to: the last source that is not the
    /// one on air now.
    ///
    /// `None` means there is nothing to go back to, which is a different
    /// answer from "go back to the slate" and is refused rather than guessed.
    pub fn previous(&self, current: Option<&str>) -> Option<Option<String>> {
        let takes = self.takes.lock();
        takes
            .iter()
            .rev()
            .find(|t| t.source.as_deref() != current)
            .map(|t| t.source.clone())
    }

    pub fn len(&self) -> usize {
        self.takes.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_are_remembered_newest_first_and_bounded() {
        let h = History::new();
        assert!(h.is_empty());
        for i in 0..(KEEP + 20) {
            h.record(Some(format!("cam{i}")), i as u64 * 100, "desk", i as u64);
        }
        assert_eq!(h.len(), KEEP);
        let recent = h.recent(3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].source.as_deref(), Some("cam119"));
        assert_eq!(recent[2].source.as_deref(), Some("cam117"));
        assert_eq!(recent[0].by, "desk");
        // A limit of zero is a mistake, not a request for nothing.
        assert_eq!(h.recent(0).len(), 1);
        assert_eq!(h.recent(1000).len(), KEEP);
    }

    /// The mixer publishes `took` for its own cuts as well as for a client's,
    /// so the same sequence number must not be written twice.
    #[test]
    fn the_same_event_is_not_recorded_twice() {
        let h = History::new();
        h.record(Some("cam1".into()), 100, "desk", 7);
        h.record(Some("cam1".into()), 100, "core", 7);
        assert_eq!(h.len(), 1);
        assert_eq!(h.recent(1)[0].by, "desk");
    }

    /// A client's take is credited to its token; a take the mixer made itself
    /// belongs to the core and says so, rather than being credited to whoever
    /// happened to call last.
    #[test]
    fn a_take_is_credited_to_whoever_claimed_it() {
        let h = History::new();
        h.expect("studio-agent");
        h.record_event(Some("cam1".into()), 100, 1);
        assert_eq!(h.recent(1)[0].by, "studio-agent");

        // The claim is spent, so the next take is the core's own.
        h.record_event(Some("cam2".into()), 200, 2);
        assert_eq!(h.recent(1)[0].by, "core");
    }

    /// Revert goes back to the last shot that is not this one, so holding a
    /// camera through three takes still reverts to the shot before it.
    #[test]
    fn revert_finds_the_shot_before_this_one() {
        let h = History::new();
        assert_eq!(h.previous(Some("cam1")), None, "nothing to go back to yet");

        h.record(Some("cam1".into()), 0, "desk", 1);
        h.record(Some("cam2".into()), 100, "desk", 2);
        assert_eq!(h.previous(Some("cam2")), Some(Some("cam1".into())));

        // Taking the same source again does not lose the shot before it.
        h.record(Some("cam2".into()), 200, "desk", 3);
        assert_eq!(h.previous(Some("cam2")), Some(Some("cam1".into())));

        // A cut to the slate is a shot like any other, and reverting from it
        // goes back to the picture.
        h.record(None, 300, "desk", 4);
        assert_eq!(h.previous(None), Some(Some("cam2".into())));
        // And reverting to the slate is a real answer, not "nothing to do".
        assert_eq!(h.previous(Some("cam2")), Some(None));
    }
}
