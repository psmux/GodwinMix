//! A reader killed mid read, and an owner that dies and comes back.

#![cfg(unix)]

mod common;

use std::time::{Duration, Instant};

use common::*;
use godwinmix_framebus::Format;

#[test]
fn child_entry() {
    child_main();
}

#[test]
fn a_second_owner_of_a_live_name_is_refused_and_a_dead_ones_socket_is_replaced() {
    let reg = registry();
    let first = publisher(&reg, small(), 2, 1);
    let opts = godwinmix_framebus::PublisherOptions::default();
    let e = godwinmix_framebus::Publisher::create(&reg, &name(), small(), opts.clone())
        .err()
        .unwrap();
    assert_eq!(e.code(), "name-taken", "{e}");
    assert!(e.to_string().contains("Subscriber"), "{e}");
    drop(first);
    // A socket file nobody answers on, as a killed owner leaves behind.
    let path = reg.path(&name()).unwrap();
    drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
    assert!(path.exists());
    let _second = godwinmix_framebus::Publisher::create(&reg, &name(), small(), opts).unwrap();
    assert_eq!(reg.list(), vec![name()]);
}

#[test]
fn a_reader_killed_while_holding_frames_leaves_nothing_locked() {
    let reg = registry();
    // One reader, one lease each: three slots, so a held slot matters.
    let mut p = publisher(&reg, small(), 1, 1);
    let mut kid = spawn("hold", &reg, "1");
    let mut seq = 0;
    let started = Instant::now();
    let holding = loop {
        seq += 1;
        p.write(None, None, |b| paint(seq, b));
        std::thread::sleep(Duration::from_millis(5));
        let s = p.stats();
        if s.readers.first().is_some_and(|r| r.holding == 1) {
            break s;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the child never took a frame"
        );
    };
    assert_eq!(holding.readers.len(), 1);
    kid.kill9();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !p.stats().readers.is_empty() {
        assert!(
            Instant::now() < deadline,
            "the dead reader still has its place"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    // With the lease gone all three slots cycle; nothing is ever dropped.
    for _ in 0..50 {
        seq += 1;
        assert!(p.write(None, None, |b| paint(seq, b)));
    }
    assert_eq!(p.stats().dropped, 0);
    // A new reader gets the place and fresh frames.
    let mut sub = subscribe(&reg);
    seq += 1;
    p.write(None, None, |b| paint(seq, b));
    let f = sub.next(Duration::from_secs(1)).unwrap().expect("a frame");
    assert_eq!(f.verify(), Some(true));
}

#[test]
fn a_reader_carries_on_when_the_owner_dies_and_another_starts_with_a_new_size() {
    let reg = registry();
    let mut first = spawn("owner", &reg, "640,360");
    num(&first.result(), "publishing");
    let mut sub = subscribe(&reg);
    let held = sub
        .next(Duration::from_secs(2))
        .unwrap()
        .expect("a frame from the first owner");
    assert_eq!(held.layout().width, 640);
    first.kill9();
    // Nothing publishes: the reader waits, and still has its frame.
    let quiet = Instant::now();
    while sub.next(Duration::from_millis(100)).unwrap().is_some() {
        assert!(
            quiet.elapsed() < Duration::from_secs(2),
            "frames after the owner died"
        );
    }
    assert_eq!(held.verify(), Some(true), "a held frame outlives its owner");
    let mut second = spawn("owner", &reg, "1280,720");
    num(&second.result(), "publishing");
    let deadline = Instant::now() + Duration::from_secs(5);
    let f = loop {
        if let Some(f) = sub.next(Duration::from_millis(200)).unwrap() {
            break f;
        }
        assert!(
            Instant::now() < deadline,
            "the reader never found the second owner"
        );
    };
    assert_eq!(
        (f.layout().format, f.layout().width, f.layout().height),
        (Format::Nv12, 1280, 720)
    );
    assert_eq!(f.verify(), Some(true));
    assert_eq!(sub.reconnects(), 1);
    second.kill9();
}
