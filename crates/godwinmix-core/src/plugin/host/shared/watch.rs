//! What a shared source measures of the bus: frames, gaps, and how long a
//! frame took from the owner's publish to this source.
//!
//! Filled from probes on streaming threads, so nothing here waits: atomics,
//! and a `try_lock` on the samples that skips a sample rather than wait for
//! the one reader of them.

use godwinmix_framebus::monotonic_ns;
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// Samples kept for the percentiles: the last few seconds at 30 fps.
const SAMPLES: usize = 512;
/// A gap longer than this is one worth naming in the log.
const NOTABLE_GAP_NS: u64 = 250_000_000;

/// The last few hundred durations of one kind, in microseconds.
#[derive(Default)]
pub struct Samples(Mutex<Vec<u32>>);

impl Samples {
    /// Record one, or skip it if the reader of them holds the lock.
    pub fn add_ns(&self, ns: u64) {
        if let Some(mut held) = self.0.try_lock() {
            if held.len() >= SAMPLES {
                held.remove(0);
            }
            held.push((ns / 1000).min(u64::from(u32::MAX)) as u32);
        }
    }

    /// `{p50, p99, max}` in milliseconds, or nulls before the first.
    pub fn report(&self) -> Value {
        let mut all = self.0.lock().clone();
        all.sort_unstable();
        let pick = |q: f64| -> Value {
            if all.is_empty() {
                return Value::Null;
            }
            let i = ((all.len() - 1) as f64 * q).round() as usize;
            json!(f64::from(all[i]) / 1000.0)
        };
        json!({"p50": pick(0.5), "p99": pick(0.99), "max": pick(1.0)})
    }
}

/// What one shared source has seen of the bus.
#[derive(Default)]
pub struct Watch {
    frames: AtomicU64,
    last_ns: AtomicU64,
    longest_gap_ns: AtomicU64,
    last_gap_ns: AtomicU64,
    gaps: AtomicU64,
    /// From the owner publishing a frame to this reader holding it.
    pub(super) hop: Samples,
    /// From the owner publishing a frame to it leaving this source for the
    /// programme, through this side's normaliser.
    out: Samples,
}

impl Watch {
    /// One frame left for the programme, published at `captured_ns`.
    pub(super) fn frame(&self, captured_ns: Option<u64>) {
        let now = monotonic_ns();
        let last = self.last_ns.swap(now, Relaxed);
        self.frames.fetch_add(1, Relaxed);
        if last != 0 {
            let gap = now.saturating_sub(last);
            self.longest_gap_ns.fetch_max(gap, Relaxed);
            if gap > NOTABLE_GAP_NS {
                self.last_gap_ns.store(gap, Relaxed);
                self.gaps.fetch_add(1, Relaxed);
            }
        }
        if let Some(at) = captured_ns {
            self.out.add_ns(now.saturating_sub(at));
        }
    }

    pub fn frames(&self) -> u64 {
        self.frames.load(Relaxed)
    }

    /// Gaps longer than a quarter of a second so far.
    pub fn gaps(&self) -> u64 {
        self.gaps.load(Relaxed)
    }

    pub fn last_gap_ms(&self) -> u64 {
        self.last_gap_ns.load(Relaxed) / 1_000_000
    }

    /// Milliseconds since the last frame, or `None` before the first.
    pub fn quiet_ms(&self) -> Option<u64> {
        let last = self.last_ns.load(Relaxed);
        (last != 0).then(|| monotonic_ns().saturating_sub(last) / 1_000_000)
    }

    /// The numbers `call("share")` answers with.
    pub fn report(&self) -> Value {
        json!({
            "frames": self.frames(),
            "longest_gap_ms": self.longest_gap_ns.load(Relaxed) as f64 / 1e6,
            "last_gap_ms": self.last_gap_ns.load(Relaxed) as f64 / 1e6,
            "gaps": self.gaps.load(Relaxed),
            "hop_ms": self.hop.report(),
            "to_programme_ms": self.out.report(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_watch_counts_frames_and_reports_percentiles() {
        let w = Watch::default();
        assert!(w.quiet_ms().is_none());
        let now = monotonic_ns();
        w.frame(Some(now.saturating_sub(2_000_000)));
        w.frame(Some(now));
        w.frame(None);
        let r = w.report();
        assert_eq!(r["frames"], 3);
        assert!(r["to_programme_ms"]["max"].as_f64().unwrap() >= 2.0, "{r}");
        assert!(r["hop_ms"]["p50"].is_null(), "{r}");
        assert!(w.quiet_ms().is_some());
    }
}
