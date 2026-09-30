//! The reader's half of the ring: lease the newest frame, give it back.

use std::sync::atomic::Ordering::{Acquire, Relaxed, SeqCst};

use super::{Lease, Ring};
use crate::header::{unpack_latest, WRITING};

/// A reader of sound this many chunks behind the newest jumps to it.
pub const CATCH_UP: u64 = 8;

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

    /// Lease the chunk after `after` for reader `reader`, for sound, which is
    /// read in order. The first read, and a reader more than [`CATCH_UP`]
    /// chunks behind, start from the newest instead: old sound is late sound.
    pub fn lease_next(&self, reader: usize, after: u64) -> Lease {
        let h = self.header();
        let latest = h.latest.load(Acquire);
        let (newest, _) = unpack_latest(latest);
        if after == 0 || newest.saturating_sub(after) > CATCH_UP {
            return self.lease_latest(reader, after);
        }
        let r = &h.readers[reader];
        loop {
            if latest == 0 || newest <= after {
                return Lease::Nothing;
            }
            if r.leases.load(Relaxed).count_ones() >= h.leases_per_reader {
                return Lease::Full;
            }
            let Some((seq, slot)) = self.oldest_after(after) else {
                return Lease::Nothing;
            };
            r.leases.fetch_or(1 << slot, SeqCst);
            if h.slots[slot].seq.load(SeqCst) == seq {
                let skipped = seq - after - 1;
                r.delivered.fetch_add(1, Relaxed);
                r.skipped.fetch_add(skipped, Relaxed);
                return Lease::Leased { slot, seq, skipped };
            }
            r.leases.fetch_and(!(1 << slot), SeqCst);
        }
    }

    /// The published slot with the smallest sequence number above `after`.
    fn oldest_after(&self, after: u64) -> Option<(u64, usize)> {
        let h = self.header();
        (0..h.n_slots as usize)
            .map(|s| (h.slots[s].seq.load(Acquire), s))
            .filter(|&(q, _)| q != WRITING && q > after)
            .min()
    }

    /// Done with `slot`.
    pub fn release(&self, reader: usize, slot: usize) {
        self.header().readers[reader]
            .leases
            .fetch_and(!(1 << slot), SeqCst);
    }
}
