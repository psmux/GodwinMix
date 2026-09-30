//! What a child process does, by role.

use std::time::{Duration, Instant};

use godwinmix_framebus::{Format, Layout, Registry};

use super::{paint, publisher, subscribe};

pub fn run(role: &str, args: &str, reg: &Registry) {
    let arg = |i: usize| args.split(',').nth(i).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    match role {
        "reader" => reader(reg, arg(0), arg(1)),
        "hold" => hold(reg, arg(0)),
        "owner" => owner(reg, arg(0) as u32, arg(1) as u32),
        other => panic!("no role {other}"),
    }
}

/// Read for `ms` milliseconds, `sleep_ms` between frames, checking every one.
fn reader(reg: &Registry, ms: u64, sleep_ms: u64) {
    let mut sub = subscribe(reg);
    println!("RESULT ready=1");
    let (mut got, mut bad, mut skipped, mut last) = (0u64, 0u64, 0u64, 0u64);
    let mut out_of_order = 0u64;
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        let Some(f) = sub.next(Duration::from_millis(100)).unwrap() else { continue };
        got += 1;
        skipped += f.skipped();
        if f.verify() != Some(true) {
            bad += 1;
        }
        if f.seq() <= last {
            out_of_order += 1;
        }
        last = f.seq();
        std::thread::sleep(Duration::from_millis(sleep_ms));
    }
    println!("RESULT got={got} bad={bad} skipped={skipped} out_of_order={out_of_order}");
}

/// Take `n` frames, keep them, say so, and wait to be killed.
fn hold(reg: &Registry, n: u64) {
    let mut sub = subscribe(reg);
    let mut held = vec![];
    while (held.len() as u64) < n {
        if let Some(f) = sub.next(Duration::from_millis(100)).unwrap() {
            held.push(f);
        }
    }
    println!("RESULT holding={}", held.len());
    std::thread::sleep(Duration::from_secs(60));
}

/// Publish frames of `width`x`height` NV12 at about 100 a second until killed.
fn owner(reg: &Registry, width: u32, height: u32) {
    let mut p = publisher(reg, Layout::new(Format::Nv12, width, height).unwrap(), 4, 2);
    println!("RESULT publishing=1");
    for seq in 1.. {
        p.write(Some(seq * 10_000_000), None, |b| paint(seq, b));
        std::thread::sleep(Duration::from_millis(10));
    }
}
