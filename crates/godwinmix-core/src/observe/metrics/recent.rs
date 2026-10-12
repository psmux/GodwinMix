//! The worst of a number over the last half minute or so, kept with atomics.
//!
//! Both programme gauges age the same way and both run on the compositor's
//! streaming thread, so this takes no lock and allocates nothing: three
//! relaxed atomics and a compare.

use std::sync::atomic::{AtomicU64, Ordering};

/// How long a bucket stays open, in nanoseconds of the probe's own clock.
///
/// The gauges answer "has the programme stalled lately", not "did it ever",
/// and the difference matters to everybody who reads them. A number that only
/// ever goes up is poisoned for the life of the process by one bad moment
/// during startup: an alert stays lit after the cause is gone, and a soak run
/// cannot say which of its rounds was the bad one. Thirty seconds, in two
/// buckets, so the answer covers between thirty and sixty seconds of history
/// and a scrape at any ordinary interval cannot miss a spike.
pub(super) const SPAN_NS: u64 = 30_000_000_000;

/// Two buckets, rolled over every [`SPAN_NS`]. The reading is the worse of
/// the one being filled and the one before it.
#[derive(Default)]
pub(super) struct Recent {
    opened: AtomicU64,
    this: AtomicU64,
    last: AtomicU64,
}

impl Recent {
    /// Fold one value in at `now` and say what the gauge should read.
    pub(super) fn observe(&self, now: u64, value: u64) -> u64 {
        if now.saturating_sub(self.opened.load(Ordering::Relaxed)) >= SPAN_NS {
            self.last.store(self.this.swap(0, Ordering::Relaxed), Ordering::Relaxed);
            self.opened.store(now, Ordering::Relaxed);
        }
        self.this
            .fetch_max(value, Ordering::Relaxed)
            .max(value)
            .max(self.last.load(Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spike_is_kept_for_one_to_two_spans_and_then_forgotten() {
        let r = Recent::default();
        assert_eq!(r.observe(SPAN_NS, 200), 200, "the spike is read at once");
        assert_eq!(r.observe(SPAN_NS + 1, 33), 200, "and held while its bucket is open");
        assert_eq!(r.observe(2 * SPAN_NS + 1, 33), 200, "and through the next bucket");
        assert_eq!(r.observe(3 * SPAN_NS + 2, 33), 33, "and gone after that");
    }
}
