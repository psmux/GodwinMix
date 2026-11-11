//! Per stage timings and the whole chain, at 320 pixels wide on one core.
//! `cargo bench -p godwinmix-track`, and with `--features tract` for the
//! model. The numbers in the README come from this file.

#[path = "../../tests/common/mod.rs"]
mod common;

use std::hint::black_box;
use std::time::Instant;

use common::{detections, paint, time, Walker};
use criterion::{criterion_group, criterion_main, Criterion};
use godwinmix_track::{
    frame_target, Choose, ChooseConfig, Chooser, CropConfig, CropSteer, Detection, Detector, FollowConfig,
    FollowOutput, Follower, FrameRef, FrameStyle, MotionDetector, PixelFormat, PtzConfig, PtzSteer, Rect, Tracker,
};

const FPS: f32 = 25.0;
const W: u32 = 320;
const H: u32 = 180;
const FRAMES: usize = 100;

fn people() -> Vec<Walker> {
    (0..5)
        .map(|i| {
            let x = 0.05 + 0.18 * i as f32;
            let c = [60 + 35 * i as u8, 200 - 30 * i as u8, 90];
            Walker::new(Rect::new(x, 0.25, 0.07, 0.55), if i % 2 == 0 { 0.06 } else { -0.05 }, c)
        })
        .collect()
}

fn painted() -> Vec<Vec<u8>> {
    let p = people();
    (0..FRAMES)
        .map(|n| {
            let mut px = Vec::new();
            paint(&p, n, FPS, W, H, &mut px);
            px
        })
        .collect()
}

fn frame(px: &[u8]) -> FrameRef<'_> {
    FrameRef::packed(W, H, PixelFormat::Rgb8, px).unwrap()
}

fn stages(c: &mut Criterion) {
    let frames = painted();
    let p = people();
    let mut n = 0usize;
    let mut next = || {
        n = (n + 1) % FRAMES;
        n
    };
    let mut det = MotionDetector::default();
    let mut out = Vec::new();
    c.bench_function("detect/motion 320x180", |b| b.iter(|| det.detect_into(&frame(&frames[next()]), &mut out)));

    let (mut tr, t0, mut dets) = (Tracker::default(), Instant::now(), Vec::<Detection>::new());
    let mut k = 0usize;
    c.bench_function("track/5 people", |b| {
        b.iter(|| {
            k += 1;
            detections(&p, k % 1000, FPS, &mut dets);
            black_box(tr.update(&dets, time(t0, k, FPS)).len())
        })
    });

    let tracks: Vec<_> = tr.tracks().to_vec();
    let (mut chooser, mut ptz, mut crop) =
        (Chooser::new(Choose::Largest, ChooseConfig::default()), PtzSteer::new(PtzConfig::default()), CropSteer::new(CropConfig::default()));
    let style = FrameStyle::default();
    c.bench_function("choose+frame+steer", |b| {
        b.iter(|| {
            k += 1;
            let t = time(t0, k, FPS);
            let choice = chooser.choose(&tracks, t);
            let shot = choice.target.and_then(|id| tracks.iter().find(|tr| tr.id == id)).map(|tr| frame_target(tr, &style, 16.0 / 9.0));
            black_box((ptz.step(shot, t), crop.step(shot, t)))
        })
    });

    let mut f = Follower::new(MotionDetector::default(), FollowConfig::default());
    let mut fo = FollowOutput::default();
    c.bench_function("chain/motion 320x180", |b| {
        b.iter(|| {
            k += 1;
            f.step_into(&frame(&frames[k % FRAMES]), time(t0, k, FPS), &mut fo)
        })
    });
    #[cfg(feature = "tract")]
    tract::stages(c);
}

#[cfg(feature = "tract")]
mod tract;

criterion_group!(benches, stages);
criterion_main!(benches);
