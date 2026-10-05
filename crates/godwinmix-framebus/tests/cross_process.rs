//! Frames cross processes intact, and a stalled reader costs nobody else.

#![cfg(unix)]

mod common;

use std::time::{Duration, Instant};

use common::*;

#[test]
fn child_entry() {
    child_main();
}

/// Publish for `ms` at about `fps`, returning the slowest single write and
/// how many frames went out. The count is what the readers are judged
/// against: a macOS runner's sleeps overran so far that 200 fps came out at
/// about 30, and a reader that saw every one of them still "got" only 41.
fn pump(p: &mut godwinmix_framebus::Publisher, ms: u64, fps: u64) -> (Duration, u64) {
    let (start, mut worst, mut seq) = (Instant::now(), Duration::ZERO, 0u64);
    while start.elapsed() < Duration::from_millis(ms) {
        seq += 1;
        let t = Instant::now();
        assert!(
            p.write(Some(seq), None, |b| paint(seq, b)),
            "the owner dropped frame {seq}"
        );
        worst = worst.max(t.elapsed());
        std::thread::sleep(Duration::from_micros(1_000_000 / fps).saturating_sub(t.elapsed()));
    }
    (worst, seq)
}

#[test]
fn every_frame_a_reader_gets_in_another_process_matches_its_checksum() {
    let reg = registry();
    let mut p = publisher(&reg, small(), 8, 2);
    let mut kids: Vec<Kid> = (0..3).map(|_| spawn("reader", &reg, "1500,0")).collect();
    for k in &mut kids {
        assert_eq!(num(&k.result(), "ready"), 1);
    }
    let mut local = subscribe(&reg);
    let inproc = std::thread::spawn(move || {
        let (mut got, mut bad) = (0, 0);
        let end = Instant::now() + Duration::from_millis(1200);
        while Instant::now() < end {
            if let Some(f) = local.next(Duration::from_millis(50)).unwrap() {
                got += 1;
                bad += (f.verify() != Some(true)) as u32;
            }
        }
        (got, bad)
    });
    // The readers listen for 1.5 s and the in process one for 1.2 s of the
    // 1.8 s written, so each should see well over half of what went out.
    let (_, sent) = pump(&mut p, 1800, 200);
    assert!(sent > 20, "only {sent} frames went out");
    for k in &mut kids {
        let r = k.result();
        assert!(num(&r, "got") * 2 > sent, "{r:?} of {sent} sent");
        assert_eq!(num(&r, "bad"), 0, "{r:?}");
        assert_eq!(num(&r, "out_of_order"), 0, "{r:?}");
    }
    let (got, bad) = inproc.join().unwrap();
    assert!(
        got * 2 > sent && bad == 0,
        "in process reader got {got} of {sent} sent, {bad} bad"
    );
    assert_eq!(p.stats().dropped, 0);
}

#[test]
fn a_stalled_reader_skips_frames_and_never_slows_the_owner_or_the_others() {
    let reg = registry();
    let mut p = publisher(&reg, small(), 4, 2);
    let mut slow = spawn("reader", &reg, "2000,400");
    let mut fast = spawn("reader", &reg, "2000,0");
    num(&slow.result(), "ready");
    num(&fast.result(), "ready");
    let (worst, sent) = pump(&mut p, 2200, 100);
    let (s, f) = (slow.result(), fast.result());
    // The slow one sleeps 400 ms a frame: about five frames in two seconds,
    // and everything else skipped, which is most of what was sent. The fast
    // one sees nearly every frame.
    assert!(
        num(&s, "got") <= 7 && num(&s, "skipped") > 3 * num(&s, "got"),
        "slow reader: {s:?} of {sent} sent"
    );
    assert!(num(&f, "got") * 2 > sent, "fast reader: {f:?} of {sent} sent");
    assert!(
        num(&f, "skipped") * 10 < num(&f, "got"),
        "fast reader: {f:?}"
    );
    assert_eq!(num(&s, "bad") + num(&f, "bad"), 0);
    assert_eq!(p.stats().dropped, 0, "the owner never ran out of slots");
    assert!(
        worst < Duration::from_millis(20),
        "one write took {worst:?}"
    );
}
