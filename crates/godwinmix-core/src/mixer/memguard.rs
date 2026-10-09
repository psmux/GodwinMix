//! The show's memory guard: an alarm, the queues named, and the source that
//! holds the most restarted, before the machine runs out.
//!
//! On 2026-10-09 a soak test of 0.3.0 watched a show's private memory grow by
//! 40 MB a second after its ingest plugin was killed, to 11.8 GB, with the
//! machine down to 811 MB free. Earlier the same day memory exhausted like
//! that made tokio panic with Windows error 1450. The five minute line in
//! `memwatch` dated it afterwards; nothing stopped it. Every queue now has a
//! backstop (`gstutil::backstop`), and this is the second line behind that:
//! for growth that is not in a queue, and for a backstop set too high for the
//! machine it runs on.
//!
//! Every five seconds the show's private memory is read on a blocking task.
//! Past the threshold (`[memory] guard_mb`, by default a quarter of the
//! machine or 4 GB, whichever is lower) the guard:
//!
//! * writes an error naming the five fullest queues in every pipeline, by
//!   bytes, with the pipeline each is in;
//! * raises a `memory` alarm (`MixerHandle::memory_alarm`), which the vitals
//!   add to the show's health and the wall shows like any other;
//! * restarts the source whose queue holds the most, when one holds at least
//!   [`BLAME_BYTES`] and can be traced to a source. A source's input pipeline
//!   is its own; a queue in the programme pipeline is traced up its chain to
//!   the source branch it hangs off. The programme itself, an output or the
//!   encoder is never restarted by this: they are named and left alone.
//!
//! After a restart it waits [`SETTLE`] before blaming anything again, and the
//! alarm clears once memory falls under nine tenths of the threshold. The
//! mixer thread only reads the clock and starts the task; nothing here runs
//! on a streaming thread.

use super::{Command, Mixer, MixerHandle};
use crate::config::memory::MemoryConfig;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

mod held;
pub use held::{fullest_queues, source_of, Held};
use held::describe;

/// How often the show's memory is read.
pub const EVERY: Duration = Duration::from_secs(5);
/// What one queue must hold before its source is restarted for it.
pub const BLAME_BYTES: u64 = 64 * 1024 * 1024;
/// How long after a restart the guard waits before restarting again.
pub const SETTLE: Duration = Duration::from_secs(30);
/// Under this share of the threshold the alarm clears.
const CLEAR_SHARE: f64 = 0.9;
/// How often the error is repeated while the show stays over.
const REPEAT: Duration = Duration::from_secs(60);
const MB: u64 = 1024 * 1024;

/// The alarm while it holds: when it began, and a sentence for a person.
#[derive(Debug, Clone, PartialEq)]
pub struct Pressure {
    pub since_ms: u64,
    pub detail: String,
}

/// What the guard keeps between checks.
#[derive(Default)]
pub struct Guard {
    pub(super) due: Option<Instant>,
    shared: Arc<Shared>,
}

#[derive(Default)]
struct Shared {
    /// A check is running; the next tick does not start another.
    busy: AtomicBool,
    /// When the guard last restarted a source, and when it last wrote the error.
    acted: Mutex<Option<Instant>>,
    said: Mutex<Option<Instant>>,
}

impl Mixer {
    /// Called from the tick. Starts a check every [`EVERY`] and returns.
    pub(super) fn guard_memory(&mut self) {
        let now = Instant::now();
        let guard = &mut self.memory.guard;
        if guard.due.is_some_and(|due| now < due) {
            return;
        }
        guard.due = Some(now + EVERY);
        let shared = guard.shared.clone();
        if shared.busy.swap(true, Ordering::AcqRel) {
            return;
        }
        let cfg = self.cfg.memory.clone();
        let handle = self.handle.clone();
        self.rt.spawn_blocking(move || {
            check(&cfg, &shared, &handle);
            shared.busy.store(false, Ordering::Release);
        });
    }
}

/// One check, on a blocking task.
fn check(cfg: &MemoryConfig, shared: &Shared, handle: &MixerHandle) {
    let Some(limit) = cfg.threshold(crate::observe::doctor::total_memory_bytes()) else {
        clear(handle, 0, 0);
        return;
    };
    let Some(bytes) = godwinmix_host::sampler::private_bytes(std::process::id()) else { return };
    if bytes < limit {
        if (bytes as f64) < limit as f64 * CLEAR_SHARE {
            clear(handle, bytes, limit);
        }
        return;
    }
    let queues = fullest_queues();
    let named = describe(&queues);
    raise(handle, bytes, limit, &queues);
    if due(&shared.said, REPEAT) {
        error!(
            private_mb = bytes / MB,
            limit_mb = limit / MB,
            queues = %named,
            "the show's memory is past its guard's threshold. The fullest queues are named \
             here; the source holding the most is restarted. Set [memory] guard_mb to move \
             the threshold"
        );
    }
    let Some((top, source)) = blame(&queues) else { return };
    if !due(&shared.acted, SETTLE) {
        return;
    }
    warn!(
        source = %source,
        queue = %top.element,
        pipeline = %top.pipeline,
        held_mb = top.bytes / MB,
        "restarting this source to give back the memory its queue holds; the programme \
         carries on with the source's last frame"
    );
    let _ = handle.send(Command::RestartSource(source.to_string()));
}

/// The fullest queue that holds at least [`BLAME_BYTES`] and belongs to a
/// source, with that source. `queues` is fullest first.
pub fn blame(queues: &[Held]) -> Option<(&Held, &str)> {
    queues.iter().take_while(|q| q.bytes >= BLAME_BYTES).find_map(|q| q.source.as_deref().map(|s| (q, s)))
}

/// Whether `at` is unset or older than `every`, and if so, set it to now.
fn due(at: &Mutex<Option<Instant>>, every: Duration) -> bool {
    let mut at = at.lock();
    let now = Instant::now();
    if at.is_some_and(|t| now.duration_since(t) < every) {
        return false;
    }
    *at = Some(now);
    true
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn raise(handle: &MixerHandle, bytes: u64, limit: u64, queues: &[Held]) {
    let mut held = handle.memory.lock();
    let since_ms = held.as_ref().map_or_else(now_ms, |p| p.since_ms);
    let top = queues.first().map_or_else(String::new, |q| {
        format!(" The fullest queue is {} in {} with {:.1} MB.", q.element, q.pipeline, q.bytes as f64 / MB as f64)
    });
    let detail = format!("The show holds {} MB, past its {} MB guard.{top}", bytes / MB, limit / MB);
    *held = Some(Pressure { since_ms, detail });
}

fn clear(handle: &MixerHandle, bytes: u64, limit: u64) {
    if handle.memory.lock().take().is_some() {
        info!(private_mb = bytes / MB, limit_mb = limit / MB, "the show's memory is back under its guard");
    }
}

#[cfg(test)]
#[path = "memguard_tests.rs"]
mod tests;
