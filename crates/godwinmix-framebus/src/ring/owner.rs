//! The owner's half of the ring: claim a slot, publish into it, and forget a
//! reader that went away. The ordering argument is at the top of mod.rs.

use std::sync::atomic::Ordering::{Relaxed, Release, SeqCst};

use super::{Meta, Ring};
use crate::header::{pack_latest, unpack_latest, MAX_SLOTS, WRITING};

impl Ring {
    fn leased(&self) -> u64 {
        self.header()
            .readers
            .iter()
            .fold(0, |m, r| m | r.leases.load(SeqCst))
    }

    /// A slot to write the next frame into, or `None` when every slot is
    /// leased, in which case the frame is counted as dropped.
    ///
    /// A picture goes in the lowest numbered free slot, which keeps the pages
    /// touched few. A chunk of sound goes in the oldest, so the chunks a reader
    /// has not reached yet are the last to be overwritten.
    pub fn claim(&self) -> Option<usize> {
        let h = self.header();
        let latest = h.latest.load(Relaxed);
        let newest = (latest != 0).then(|| unpack_latest(latest).1);
        let busy = self.leased();
        let n = h.n_slots as usize;
        let mut order = [0usize; MAX_SLOTS];
        order.iter_mut().enumerate().for_each(|(i, s)| *s = i);
        let order = &mut order[..n];
        if self.in_order() {
            order.sort_by_key(|&s| h.slots[s].seq.load(Relaxed));
        }
        for &s in order.iter() {
            if Some(s) == newest || busy & (1 << s) != 0 {
                continue;
            }
            let old = h.slots[s].seq.load(Relaxed);
            h.slots[s].seq.store(WRITING, SeqCst);
            if self.leased() & (1 << s) == 0 {
                return Some(s);
            }
            h.slots[s].seq.store(old, SeqCst);
        }
        h.dropped.fetch_add(1, Relaxed);
        None
    }

    /// Make the frame in `slot` the newest, as number `seq`.
    pub fn publish(&self, slot: usize, seq: u64, meta: Meta, now_ns: u64) {
        let h = self.header();
        let s = &h.slots[slot];
        s.pts.store(meta.pts, Relaxed);
        s.duration.store(meta.duration, Relaxed);
        s.captured_ns.store(meta.captured_ns, Relaxed);
        s.published_ns.store(now_ns, Relaxed);
        s.checksum.store(meta.checksum, Relaxed);
        s.len.store(meta.len, Relaxed);
        s.seq.store(seq, Release);
        h.latest.store(pack_latest(seq, slot), Release);
        h.published.fetch_add(1, Relaxed);
    }

    /// Give back a claimed slot without publishing it.
    pub fn abandon(&self, slot: usize) {
        self.header().slots[slot].seq.store(0, Release);
    }

    /// A reader place that nobody holds, marked as held.
    pub fn take_reader(&self, pid: u32) -> Option<usize> {
        let h = self.header();
        let i = h
            .readers
            .iter()
            .position(|r| r.live.compare_exchange(0, 1, SeqCst, Relaxed).is_ok())?;
        let r = &h.readers[i];
        r.pid.store(pid, Relaxed);
        r.delivered.store(0, Relaxed);
        r.skipped.store(0, Relaxed);
        Some(i)
    }

    /// A reader went away; free its place and every lease it held.
    pub fn reset_reader(&self, reader: usize) {
        let r = &self.header().readers[reader];
        r.leases.store(0, SeqCst);
        r.live.store(0, Release);
    }
}
