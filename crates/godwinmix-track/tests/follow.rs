//! The whole chain on painted scenes with the motion backend, which is what
//! CI runs: no model, no network.

mod common;

use std::time::Instant;

use common::{paint, time, Walker};
use godwinmix_track::{
    Class, Detector, FollowConfig, FollowOutput, FollowState, Follower, FrameRef, Framing, MotionConfig,
    MotionDetector, PixelFormat, Rect,
};

const FPS: f32 = 25.0;
const W: u32 = 320;
const H: u32 = 180;

fn walker() -> Walker {
    Walker::new(Rect::new(0.15, 0.25, 0.08, 0.6), 0.1, [200, 60, 40])
}

#[test]
fn motion_finds_a_walking_person_and_ignores_a_still_frame() {
    let (mut det, mut px) = (MotionDetector::new(MotionConfig::default()), Vec::new());
    let w = [walker()];
    let mut found = Vec::new();
    for n in 0..20 {
        paint(&w, n, FPS, W, H, &mut px);
        let f = FrameRef::packed(W, H, PixelFormat::Rgb8, &px).unwrap();
        det.detect_into(&f, &mut found);
    }
    assert_eq!(found.len(), 1, "{found:?}");
    let (d, truth) = (found[0], w[0].at(19, FPS));
    assert_eq!(d.class, Class::Motion);
    assert!(d.rect.iou(&truth) > 0.4, "{:?} against {truth:?}", d.rect);
    // The background learns a person who stops.
    for _ in 0..200 {
        paint(&w, 19, FPS, W, H, &mut px);
        let f = FrameRef::packed(W, H, PixelFormat::Rgb8, &px).unwrap();
        det.detect_into(&f, &mut found);
    }
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_light_change_resets_rather_than_detecting_the_whole_frame() {
    let mut det = MotionDetector::new(MotionConfig::default());
    let dark = vec![40u8; (W * H) as usize];
    let bright = vec![200u8; (W * H) as usize];
    for px in [&dark, &dark, &bright, &bright] {
        let f = FrameRef::packed(W, H, PixelFormat::Gray8, px).unwrap();
        assert!(det.detect(&f).is_empty());
    }
}

#[test]
fn the_follower_crops_onto_the_person_and_follows_them() {
    let cfg = FollowConfig { detect_every: 2, ..FollowConfig::default() };
    let mut f = Follower::new(MotionDetector::default(), cfg);
    f.set_framing(Framing::Full);
    let (w, t0, mut px, mut out) = ([walker()], Instant::now(), Vec::new(), FollowOutput::default());
    for n in 0..100 {
        paint(&w, n, FPS, W, H, &mut px);
        let frame = FrameRef::packed(W, H, PixelFormat::Rgb8, &px).unwrap();
        f.step_into(&frame, time(t0, n, FPS), &mut out);
        assert_eq!(out.detected, n % 2 == 0);
    }
    let truth = w[0].at(99, FPS);
    assert_eq!(out.state, FollowState::Following);
    assert!(out.target.is_some() && out.tracks.len() == 1);
    assert!(out.crop.contains(truth.center()), "{:?} should hold {truth:?}", out.crop);
    assert!(out.crop.h < 1.0 && out.crop.h > truth.h, "{:?}", out.crop);
    assert!(out.ptz.pan > 0.0, "the person walks right of centre: {:?}", out.ptz);
}

#[test]
fn a_bad_frame_is_skipped_not_a_panic() {
    let mut f = Follower::new(MotionDetector::default(), FollowConfig::default());
    let frame = FrameRef { width: 320, height: 180, stride: 320, format: PixelFormat::Nv12, data: &[0; 100] };
    let out = f.step(&frame, Instant::now());
    assert!(!out.detected && out.tracks.is_empty());
}

#[test]
fn a_boxed_detector_works_as_the_default_follower() {
    let det: Box<dyn Detector + Send> = Box::new(MotionDetector::default());
    let mut f: Follower = Follower::new(det, FollowConfig::default());
    let px = vec![0u8; (W * H * 4) as usize];
    let frame = FrameRef::packed(W, H, PixelFormat::Bgra8, &px).unwrap();
    assert_eq!(f.step(&frame, Instant::now()).state, FollowState::Idle);
}
