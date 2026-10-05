//! The mixer's own memory, written to the log every five minutes.
//!
//! On 2026-10-05 the desktop app's mixer grew to about 12 GB over an hour and
//! died with `memory allocation of 3110409 bytes failed`. That line was the
//! first in the log to mention memory at all, and the only way to see when the
//! growth started was to line the stall reports up against the crash and
//! guess. One line every five minutes would have dated it, and said how many
//! sources were running when it began.
//!
//! The cost is one read of the process's resident size on the supervisor's
//! existing tick, every five minutes, on a blocking task so that the `ps` a
//! macOS sample starts never holds the mixer thread. No thread of its own,
//! nothing per frame, nothing while the mixer is not running. The reading is
//! the same one `plugin.stats` gives each plugin (`godwinmix_host::sampler`):
//! the working set on Windows and the resident set elsewhere.

use super::Mixer;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// How often the mixer writes its memory down.
pub const EVERY: Duration = Duration::from_secs(300);

/// Consecutive rising samples that count as steady growth: half an hour.
pub const RISING: usize = 6;

/// How much those samples must have grown by, in total, before a warning. A
/// mosaic subscriber or a new source adds tens of megabytes and stays; this is
/// well past that, and well short of the gigabytes that end a process.
pub const WARN_GROWTH: u64 = 512 * 1024 * 1024;

/// What the watch keeps between samples.
#[derive(Default)]
pub struct MemoryWatch {
    due: Option<Instant>,
    trend: Arc<Mutex<Trend>>,
}

/// The last few readings, oldest first.
#[derive(Default)]
pub struct Trend {
    samples: VecDeque<u64>,
}

impl Trend {
    /// Add a reading. Returns the growth over the window when every reading
    /// in it rose and the total is past `WARN_GROWTH`. The window then starts
    /// again, so a process that keeps growing is warned about once per half
    /// hour rather than once per sample.
    pub fn push(&mut self, bytes: u64) -> Option<u64> {
        self.samples.push_back(bytes);
        while self.samples.len() > RISING + 1 {
            self.samples.pop_front();
        }
        if self.samples.len() <= RISING {
            return None;
        }
        let rising = self.samples.iter().zip(self.samples.iter().skip(1)).all(|(a, b)| b > a);
        let grew = bytes.saturating_sub(*self.samples.front()?);
        if rising && grew >= WARN_GROWTH {
            self.samples.clear();
            self.samples.push_back(bytes);
            return Some(grew);
        }
        None
    }
}

impl Mixer {
    /// Called from the tick. Does nothing until the next sample is due.
    pub(super) fn watch_memory(&mut self) {
        let now = Instant::now();
        if self.memory.due.is_some_and(|due| now < due) {
            return;
        }
        self.memory.due = Some(now + EVERY);
        let sources = self.sources.len();
        let outputs = self.outputs.len();
        let trend = self.memory.trend.clone();
        self.rt.spawn_blocking(move || {
            let pid = std::process::id();
            let sample = godwinmix_host::sampler::Sampler::new().sample(&[pid]);
            let Some(bytes) = sample.get(&pid).and_then(|s| s.rss_bytes) else { return };
            let mb = bytes / (1024 * 1024);
            info!(resident_mb = mb, sources, outputs, "mixer memory");
            if let Some(grew) = trend.lock().push(bytes) {
                warn!(
                    resident_mb = mb,
                    grew_mb = grew / (1024 * 1024),
                    minutes = (EVERY * RISING as u32).as_secs() / 60,
                    sources,
                    "the mixer's memory has grown at every sample for half an hour; \
                     the stall reports and restarts above say which source was busy"
                );
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1024 * 1024;

    #[test]
    fn steady_growth_past_the_bar_warns_once() {
        let mut t = Trend::default();
        let warned: Vec<_> = (0..=RISING as u64).map(|n| t.push(1000 * MB + n * 100 * MB)).collect();
        assert_eq!(warned.last().copied().flatten(), Some(600 * MB));
        assert!(warned[..RISING].iter().all(Option::is_none), "warned before the window was full");
        assert_eq!(t.push(1700 * MB), None, "warned again on the very next sample");
    }

    #[test]
    fn a_dip_or_a_small_rise_says_nothing() {
        let mut t = Trend::default();
        let dipped = [1000, 1200, 1400, 1300, 1600, 1800, 2000].map(|m| t.push(m * MB));
        assert!(dipped.iter().all(Option::is_none), "a dip is not steady growth");
        let mut t = Trend::default();
        let small = (0..=RISING as u64).map(|n| t.push(1000 * MB + n * MB));
        assert!(small.into_iter().all(|w| w.is_none()), "six megabytes is not a leak worth a warning");
    }
}
