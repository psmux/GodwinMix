//! The plan's budget, checked: nothing allocates per frame after warm up,
//! and the full chain at 320 pixels wide stays under 15 ms a frame on one
//! core. One test per file, because the allocation count is process wide.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use common::{paint, time, Walker};
use godwinmix_track::{FollowConfig, FollowOutput, Follower, FrameRef, MotionDetector, PixelFormat, Rect};

struct Counting;

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }

    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }

    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
}

#[global_allocator]
static A: Counting = Counting;

#[test]
fn no_allocation_per_frame_and_under_15_ms() {
    const FPS: f32 = 25.0;
    let (w, h) = (320u32, 180u32);
    let people = [
        Walker::new(Rect::new(0.1, 0.25, 0.08, 0.6), 0.08, [200, 60, 40]),
        Walker::new(Rect::new(0.7, 0.3, 0.07, 0.55), -0.05, [40, 60, 200]).hide(120, 160),
    ];
    // Paint every frame first, so the loop measures only the engine.
    let frames: Vec<Vec<u8>> = (0..300)
        .map(|n| {
            let mut px = Vec::new();
            paint(&people, n, FPS, w, h, &mut px);
            px
        })
        .collect();
    let mut f = Follower::new(MotionDetector::default(), FollowConfig::default());
    let (t0, mut out) = (Instant::now(), FollowOutput::default());
    let mut run = |range: std::ops::Range<usize>| {
        let start = Instant::now();
        for n in range.clone() {
            let frame = FrameRef::packed(w, h, PixelFormat::Rgb8, &frames[n]).unwrap();
            f.step_into(&frame, time(t0, n, FPS), &mut out);
        }
        start.elapsed() / range.len() as u32
    };
    run(0..60);
    let before = ALLOCS.load(Ordering::Relaxed);
    let per_frame = run(60..300);
    let allocs = ALLOCS.load(Ordering::Relaxed) - before;
    assert_eq!(allocs, 0, "{allocs} allocations in 240 frames after warm up");
    if cfg!(debug_assertions) {
        eprintln!("skipping the time budget in a debug build ({per_frame:?} a frame); run with --release");
        return;
    }
    assert!(per_frame < Duration::from_millis(15), "{per_frame:?} a frame, the budget is 15 ms");
}
