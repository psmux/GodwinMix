//! Who is watching, and how much is going out.
//!
//! A viewer is a client that fetched a segment or a part in the last two
//! windows. A player is told who it is by the `v` the multivariant playlist
//! puts on every URI it hands out; one that opened a media playlist directly
//! is known by its address and user agent instead.
//!
//! Egress is counted per second in a small ring of buckets, and read as the
//! average of the last few whole seconds. Both locks are held for a map
//! insert or an add, never across anything that waits.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const BUCKETS: usize = 8;

pub struct Viewers {
    horizon: Duration,
    seen: Mutex<HashMap<String, Instant>>,
    meter: Mutex<Meter>,
    total: AtomicU64,
}

struct Meter {
    started: Instant,
    /// (second since start, bytes in that second)
    buckets: [(u64, u64); BUCKETS],
}

impl Viewers {
    /// `horizon` is how long a viewer counts after its last fetch.
    pub fn new(horizon: Duration) -> Viewers {
        Viewers {
            horizon,
            seen: Mutex::new(HashMap::new()),
            meter: Mutex::new(Meter { started: Instant::now(), buckets: [(u64::MAX, 0); BUCKETS] }),
            total: AtomicU64::new(0),
        }
    }

    /// A segment, part or init of `bytes` went to `who`.
    pub fn served(&self, who: &str, bytes: usize) {
        self.served_at(who, bytes, Instant::now());
    }

    fn served_at(&self, who: &str, bytes: usize, now: Instant) {
        {
            let mut seen = self.seen.lock();
            match seen.get_mut(who) {
                Some(t) => *t = now,
                None => {
                    seen.insert(who.to_string(), now);
                }
            }
        }
        self.total.fetch_add(bytes as u64, Ordering::Relaxed);
        self.meter.lock().add(now, bytes as u64);
    }

    /// Viewers seen within the horizon. Forgets the rest.
    pub fn count(&self) -> usize {
        self.count_at(Instant::now())
    }

    fn count_at(&self, now: Instant) -> usize {
        let mut seen = self.seen.lock();
        seen.retain(|_, t| now.saturating_duration_since(*t) <= self.horizon);
        seen.len()
    }

    /// Average egress over the last few whole seconds, in kbit/s.
    pub fn egress_kbps(&self) -> u32 {
        self.meter.lock().kbps(Instant::now())
    }

    /// Bytes sent since the output started.
    pub fn total_bytes(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }
}

impl Meter {
    fn second(&self, now: Instant) -> u64 {
        now.saturating_duration_since(self.started).as_secs()
    }

    fn add(&mut self, now: Instant, bytes: u64) {
        let s = self.second(now);
        let slot = &mut self.buckets[(s as usize) % BUCKETS];
        if slot.0 != s {
            *slot = (s, 0);
        }
        slot.1 += bytes;
    }

    /// The whole seconds before this one, up to `BUCKETS - 1` of them.
    fn kbps(&self, now: Instant) -> u32 {
        let s = self.second(now);
        let span = (BUCKETS as u64 - 1).min(s);
        if span == 0 {
            return 0;
        }
        let bytes: u64 = self
            .buckets
            .iter()
            .filter(|(sec, _)| *sec != u64::MAX && *sec < s && *sec >= s - span)
            .map(|(_, b)| b)
            .sum();
        (bytes * 8 / 1000 / span) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_viewer_counts_until_the_horizon_passes() {
        let v = Viewers::new(Duration::from_secs(60));
        let t = Instant::now();
        v.served_at("a", 10, t);
        v.served_at("b", 10, t + Duration::from_secs(30));
        v.served_at("a", 10, t + Duration::from_secs(40));
        assert_eq!(v.count_at(t + Duration::from_secs(50)), 2);
        assert_eq!(v.count_at(t + Duration::from_secs(95)), 1, "b was last seen at 30 s");
        assert_eq!(v.count_at(t + Duration::from_secs(101)), 0);
        assert_eq!(v.total_bytes(), 30);
    }

    #[test]
    fn egress_is_the_average_of_whole_seconds() {
        let v = Viewers::new(Duration::from_secs(60));
        let base = v.meter.lock().started;
        for s in 0..10u64 {
            // 125 000 bytes a second is 1000 kbit/s.
            v.served_at("a", 125_000, base + Duration::from_millis(s * 1000 + 500));
        }
        let kbps = v.meter.lock().kbps(base + Duration::from_millis(10_200));
        assert_eq!(kbps, 1000);
        let later = v.meter.lock().kbps(base + Duration::from_secs(60));
        assert_eq!(later, 0, "nothing sent for a minute");
    }
}
