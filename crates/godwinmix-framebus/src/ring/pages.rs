//! The region's pages are allocated only where frames are written.

use super::tests::put;
use super::{Lease, Ring};
use crate::format::{Format, Layout};

/// Slots whose first page the process has actually touched.
fn resident_slots(r: &Ring) -> usize {
    let page = crate::shm::page_size();
    (0..r.header().n_slots as usize)
        .filter(|&s| {
            let mut v = [0u8; 1];
            // SAFETY: one mapped page at the start of a slot, and a one byte
            // vector for it.
            let rc = unsafe { libc::mincore(r.data(s).cast(), page, v.as_mut_ptr().cast()) };
            rc == 0 && v[0] & 1 != 0
        })
        .count()
}

#[test]
fn a_ring_whose_readers_keep_up_touches_only_a_few_slots() {
    let r = Ring::create(&Layout::new(Format::Nv12, 1920, 1080).unwrap(), 8, 3).unwrap();
    assert_eq!(r.header().n_slots, 26);
    let me = r.take_reader(1).unwrap();
    let mut last = 0;
    for seq in 1..=200 {
        put(&r, seq).unwrap();
        if let Lease::Leased { slot, seq, .. } = r.lease_latest(me, last) {
            last = seq;
            r.release(me, slot);
        }
    }
    let touched = resident_slots(&r);
    assert!(touched <= 3, "{touched} of 26 slots have pages");
}
