//! The reader's half of the ring: lease the newest frame, give it back.

use std::sync::atomic::Ordering::{Acquire, Relaxed, SeqCst};

use super::{Lease, Ring};
use crate::header::unpack_latest;

impl Ring {
    /// Lease the newest frame for reader `reader` if it is newer than `after`.
    pub fn lease_latest(&self, reader: usize, after: u64) -> Lease {
        let h = self.header();
        let r = &h.readers[reader];
        loop {
            let latest = h.latest.load(Acquire);
            let (seq, slot) = unpack_latest(latest);
            if latest == 0 || seq <= after {
                return Lease::Nothing;
            }
            if r.leases.load(Relaxed).count_ones() >= h.leases_per_reader {
                return Lease::Full;
            }
            r.leases.fetch_or(1 << slot, SeqCst);
            if h.slots[slot].seq.load(SeqCst) == seq {
                let skipped = if after == 0 { 0 } else { seq - after - 1 };
                r.delivered.fetch_add(1, Relaxed);
                r.skipped.fetch_add(skipped, Relaxed);
                return Lease::Leased { slot, seq, skipped };
            }
            r.leases.fetch_and(!(1 << slot), SeqCst);
        }
    }

    /// Done with `slot`.
    pub fn release(&self, reader: usize, slot: usize) {
        self.header().readers[reader]
            .leases
            .fetch_and(!(1 << slot), SeqCst);
    }
}
