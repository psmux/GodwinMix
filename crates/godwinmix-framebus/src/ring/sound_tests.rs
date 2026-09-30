//! Sound is read in order and overwritten oldest first.

use super::reader::CATCH_UP;
use super::tests::put;
use super::{Lease, Ring};
use crate::format::{Format, Layout};

fn sound(readers: usize, leases: usize) -> Ring {
    Ring::create(&Layout::audio(Format::F32, 48_000, 2).unwrap(), readers, leases).unwrap()
}

fn next(r: &Ring, me: usize, after: u64) -> (u64, u64) {
    match r.lease_next(me, after) {
        Lease::Leased { slot, seq, skipped } => {
            r.release(me, slot);
            (seq, skipped)
        }
        other => panic!("after {after}: {other:?}"),
    }
}

#[test]
fn a_reader_of_sound_gets_every_chunk_in_order() {
    let r = sound(2, 2);
    assert!(r.in_order());
    let me = r.take_reader(1).unwrap();
    put(&r, 1).unwrap();
    assert_eq!(next(&r, me, 0), (1, 0));
    for seq in 2..=5 {
        put(&r, seq).unwrap();
    }
    for want in 2..=5 {
        assert_eq!(next(&r, me, want - 1), (want, 0));
    }
    assert_eq!(r.lease_next(me, 5), Lease::Nothing);
}

#[test]
fn the_owner_overwrites_the_oldest_chunk_and_a_late_reader_skips_ahead() {
    let r = sound(4, 4);
    let slots = r.header().n_slots as u64;
    let me = r.take_reader(1).unwrap();
    put(&r, 1).unwrap();
    assert_eq!(next(&r, me, 0), (1, 0));
    // Fill every slot and one more: chunk 1's slot is the oldest and goes.
    for seq in 2..=slots + 1 {
        put(&r, seq).unwrap();
    }
    let slot_of = |seq: u64| (0..slots as usize).find(|&s| r.slot(s).seq.load(std::sync::atomic::Ordering::Relaxed) == seq);
    assert!(slot_of(1).is_none(), "the oldest chunk was the one overwritten");
    assert!(slot_of(2).is_some(), "every newer chunk is still there");
    // Two behind: in order, nothing skipped.
    assert_eq!(next(&r, me, slots - 1), (slots, 0));
    // Far behind: straight to the newest, and the gap is counted.
    let (seq, skipped) = next(&r, me, slots - CATCH_UP - 2);
    assert_eq!(seq, slots + 1);
    assert_eq!(skipped, CATCH_UP + 2);
}
