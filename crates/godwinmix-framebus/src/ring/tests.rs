use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

use super::{Lease, Meta, Ring};
use crate::format::{Format, Layout};
use crate::header::checksum;

pub fn ring(readers: usize, leases: usize) -> Ring {
    Ring::create(&Layout::new(Format::Nv12, 64, 16).unwrap(), readers, leases).unwrap()
}

pub fn put(r: &Ring, seq: u64) -> Option<usize> {
    let s = r.claim()?;
    // SAFETY: the slot is claimed, so no reader leases it.
    let bytes =
        unsafe { std::slice::from_raw_parts_mut(r.data(s), r.header().frame_size as usize) };
    bytes.fill(seq as u8);
    let meta = Meta {
        checksum: checksum(bytes),
        ..Meta::default()
    };
    r.publish(s, seq, meta, 0);
    Some(s)
}

#[test]
fn a_reader_gets_the_newest_frame_and_counts_what_it_skipped() {
    let r = ring(2, 2);
    let me = r.take_reader(1).unwrap();
    assert_eq!(r.lease_latest(me, 0), Lease::Nothing);
    put(&r, 1).unwrap();
    let Lease::Leased {
        slot,
        seq: 1,
        skipped: 0,
    } = r.lease_latest(me, 0)
    else {
        panic!()
    };
    r.release(me, slot);
    for seq in 2..=5 {
        put(&r, seq).unwrap();
    }
    let Lease::Leased {
        seq: 5, skipped: 3, ..
    } = r.lease_latest(me, 1)
    else {
        panic!()
    };
    assert_eq!(r.header().readers[me].skipped.load(Relaxed), 3);
}

#[test]
fn a_leased_slot_is_never_written_and_a_full_ring_drops_instead_of_waiting() {
    let r = ring(1, 1);
    assert_eq!(r.header().n_slots, 3);
    let readers: Vec<usize> = (0..3).map(|_| r.take_reader(1).unwrap()).collect();
    let mut held = vec![];
    for (i, &me) in readers.iter().enumerate() {
        put(&r, i as u64 + 1).unwrap();
        let Lease::Leased { slot, .. } = r.lease_latest(me, 0) else {
            panic!()
        };
        held.push(slot);
    }
    assert_eq!(
        put(&r, 4),
        None,
        "every slot is leased, so the frame is dropped"
    );
    assert_eq!(r.header().dropped.load(Relaxed), 1);
    for (i, &s) in held.iter().enumerate() {
        assert!(r.frame(s).iter().all(|&b| b == i as u8 + 1));
    }
    r.reset_reader(readers[0]);
    assert!(
        put(&r, 5).is_some(),
        "a dead reader's lease is freed with its place"
    );
}

#[test]
fn one_reader_cannot_hold_more_than_its_share() {
    let r = ring(4, 2);
    let me = r.take_reader(1).unwrap();
    put(&r, 1).unwrap();
    assert!(matches!(r.lease_latest(me, 0), Lease::Leased { .. }));
    put(&r, 2).unwrap();
    assert!(matches!(r.lease_latest(me, 1), Lease::Leased { .. }));
    put(&r, 3).unwrap();
    assert_eq!(r.lease_latest(me, 2), Lease::Full);
}

#[test]
fn readers_on_other_threads_never_see_a_torn_frame() {
    let r = Arc::new(ring(4, 2));
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    // How many readers have read at least one frame. On a two core runner the
    // writer could finish all its frames before a reader thread had been
    // scheduled, and that reader saw nothing through no fault of the ring.
    let reading = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let readers: Vec<_> = (0..4)
        .map(|_| {
            let (r, stop, reading) = (r.clone(), stop.clone(), reading.clone());
            std::thread::spawn(move || {
                let me = r.take_reader(1).unwrap();
                let (mut last, mut seen) = (0, 0u64);
                while !stop.load(Relaxed) {
                    if let Lease::Leased { slot, seq, .. } = r.lease_latest(me, last) {
                        let want = r.slot(slot).checksum.load(Relaxed);
                        assert_eq!(checksum(r.frame(slot)), want, "frame {seq} torn");
                        assert!(r.frame(slot).iter().all(|&b| b == seq as u8));
                        r.release(me, slot);
                        if seen == 0 {
                            reading.fetch_add(1, Relaxed);
                        }
                        (last, seen) = (seq, seen + 1);
                    }
                }
                seen
            })
        })
        .collect();
    let started = std::time::Instant::now();
    let mut seq = 0;
    while seq < 20_000 || (reading.load(Relaxed) < 4 && started.elapsed() < std::time::Duration::from_secs(10)) {
        seq += 1;
        put(&r, seq);
    }
    stop.store(true, Relaxed);
    for t in readers {
        assert!(t.join().unwrap() > 0);
    }
}
