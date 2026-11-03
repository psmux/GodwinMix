//! Which source, if any, the guard restarts.
//!
//! Only a queue full by its backstop is a source's fault: what it holds has
//! stopped counting in time, so its time limit never stopped it. A queue full
//! to its time limit is doing its job while the programme below it has fallen
//! behind, and every source has one like it. Restarting the source above it
//! frees a second of frames for the moment the restart takes and costs the
//! show that source. In the 0.3.25 test it was worse than that: the guard
//! restarted the newsroom set and the guest in turn every half minute for
//! seven minutes, each restart waited 30 s on a pipeline blocked behind its
//! own full queue, the old pipeline was left running beside the new one, and
//! the memory it was meant to free went up.
//!
//! So a queue full by time is never blamed, and a source the guard restarted
//! is left alone for [`AGAIN_AFTER`]: one restart frees what a stuck source
//! holds, and a second within minutes says the restart is not the cure.

use super::Held;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// What one queue must hold before its source is restarted for it.
pub const BLAME_BYTES: u64 = 64 * 1024 * 1024;
/// How long a source the guard restarted is left alone afterwards.
pub const AGAIN_AFTER: Duration = Duration::from_secs(300);

/// The fullest queue that holds at least [`BLAME_BYTES`], is full by its
/// backstop and belongs to a source `rested` allows, with that source.
/// `queues` is fullest first.
pub fn blame(queues: &[Held], rested: impl Fn(&str) -> bool) -> Option<(&Held, &str)> {
    queues
        .iter()
        .take_while(|q| q.bytes >= BLAME_BYTES)
        .filter(|q| q.backstop)
        .find_map(|q| q.source.as_deref().filter(|s| rested(s)).map(|s| (q, s)))
}

/// Whether the queues over [`BLAME_BYTES`] are all full by time: the
/// programme is behind, and no source is to blame.
pub fn all_by_time(queues: &[Held]) -> bool {
    let big: Vec<&Held> = queues.iter().take_while(|q| q.bytes >= BLAME_BYTES).collect();
    !big.is_empty() && big.iter().all(|q| !q.backstop)
}

/// When the guard last restarted each source.
#[derive(Default)]
pub struct Restarts(Mutex<HashMap<String, Instant>>);

impl Restarts {
    /// Whether `source` has not been restarted by the guard in [`AGAIN_AFTER`].
    pub fn rested(&self, source: &str) -> bool {
        self.0.lock().get(source).is_none_or(|at| at.elapsed() >= AGAIN_AFTER)
    }

    pub fn note(&self, source: &str) {
        let mut map = self.0.lock();
        map.retain(|_, at| at.elapsed() < AGAIN_AFTER);
        map.insert(source.to_string(), Instant::now());
    }
}
