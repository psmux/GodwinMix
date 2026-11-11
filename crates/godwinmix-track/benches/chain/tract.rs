//! The model backends on the committed photograph of two people.

use std::time::Instant;

use criterion::Criterion;
use godwinmix_track::{Detector, FollowConfig, FollowOutput, Follower, FrameRef, PixelFormat};

use crate::common::time;
use crate::FPS;

fn photo() -> Vec<u8> {
    // The committed photograph of two people, scaled to 320 by 240 grey.
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/images/two-people.jpg");
    let img = image::open(path).unwrap().to_luma8();
    let src = FrameRef::packed(img.width(), img.height(), PixelFormat::Gray8, img.as_raw()).unwrap();
    let mut px = vec![0; 320 * 240];
    src.downscale_luma(&mut px, 320, 240);
    px
}

pub fn stages(c: &mut Criterion) {
    use godwinmix_track::detect::tract::{PoseDetector, TractConfig, TractDetector};
    let px = photo();
    let still = || FrameRef::packed(320, 240, PixelFormat::Gray8, &px).unwrap();
    let cfg = |input| TractConfig { input, ..TractConfig::default() };
    let mut out = Vec::new();
    for input in [(320, 256), (320, 320), (416, 416)] {
        let Ok(mut det) = TractDetector::new(cfg(input)) else {
            eprintln!("no person model on this machine; skipping the tract benches");
            return;
        };
        let name = format!("detect/tract yolox-nano {}x{}", input.0, input.1);
        c.bench_function(&name, |b| b.iter(|| det.detect_into(&still(), &mut out)));
    }
    if let Ok(mut pose) = PoseDetector::new(TractDetector::new(cfg((320, 256))).unwrap(), 1, None) {
        c.bench_function("detect/tract yolox-nano 320x256 + movenet 1 person", |b| {
            b.iter(|| pose.detect_into(&still(), &mut out))
        });
    }
    let mut pose = PoseDetector::new(TractDetector::new(cfg((320, 256))).unwrap(), 1, None).unwrap();
    pose.every = 3;
    let mut f = Follower::new(pose, FollowConfig::default());
    let (t0, mut fo, mut k) = (Instant::now(), FollowOutput::default(), 0);
    c.bench_function("chain/tract 320x256 + movenet every third frame", |b| {
        b.iter(|| {
            k += 1;
            f.step_into(&still(), time(t0, k, FPS), &mut fo)
        })
    });
    for every in [1, 3] {
        let det = TractDetector::new(cfg((320, 256))).unwrap();
        let mut f = Follower::new(det, FollowConfig { detect_every: every, ..FollowConfig::default() });
        let (t0, mut fo, mut k) = (Instant::now(), FollowOutput::default(), 0);
        c.bench_function(&format!("chain/tract 320x256 detect every {every}"), |b| {
            b.iter(|| {
                k += 1;
                f.step_into(&still(), time(t0, k, FPS), &mut fo)
            })
        });
        assert!(fo.tracks.len() >= 2, "the chain sees both people: {:?}", fo.tracks);
    }
}
