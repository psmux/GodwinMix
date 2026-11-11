//! The ONNX detector on committed photographs with known people in them.
//! Needs the model: fetched on first run, and the test says so and passes
//! when this machine cannot fetch it.
#![cfg(feature = "tract")]

use std::time::Instant;

use godwinmix_track::detect::tract::{TractConfig, TractDetector};
use godwinmix_track::{Detector, FollowConfig, Follower, FrameRef, PixelFormat};

fn detector(input: (usize, usize)) -> Option<TractDetector> {
    match TractDetector::new(TractConfig { input, ..TractConfig::default() }) {
        Ok(d) => Some(d),
        Err(e) => {
            eprintln!("skipping: no person model on this machine: {e}");
            None
        }
    }
}

fn grey(name: &str) -> (Vec<u8>, u32, u32) {
    let path = format!("{}/tests/images/{name}", env!("CARGO_MANIFEST_DIR"));
    let img = image::open(&path).expect("the test image is committed").to_luma8();
    let (w, h) = img.dimensions();
    (img.into_raw(), w, h)
}

#[test]
fn finds_both_people_in_each_photograph() {
    let Some(mut det) = detector((416, 416)) else { return };
    for name in ["two-people.jpg", "two-people-2.jpg"] {
        let (px, w, h) = grey(name);
        let frame = FrameRef::packed(w, h, PixelFormat::Gray8, &px).unwrap();
        let start = Instant::now();
        let mut found = det.detect(&frame);
        eprintln!("{name}: {} people in {:?}", found.len(), start.elapsed());
        found.retain(|d| d.score > 0.5);
        found.sort_by(|a, b| a.rect.cx().total_cmp(&b.rect.cx()));
        assert_eq!(found.len(), 2, "{name}: {found:?}");
        let (left, right) = (found[0].rect, found[1].rect);
        assert!(left.x > 0.02 && left.right() < 0.3 && left.h > 0.7, "{name}: left {left:?}");
        assert!(right.x > 0.6 && right.x < 0.75 && right.h > 0.7, "{name}: right {right:?}");
    }
}

#[test]
fn an_empty_frame_has_nobody_in_it() {
    let Some(mut det) = detector((320, 320)) else { return };
    let px = vec![128u8; 320 * 180];
    let frame = FrameRef::packed(320, 180, PixelFormat::Gray8, &px).unwrap();
    assert!(det.detect(&frame).is_empty());
}

#[test]
fn the_follower_frames_the_biggest_person() {
    let Some(det) = detector((320, 320)) else { return };
    let mut f = Follower::new(det, FollowConfig::default());
    let (px, w, h) = grey("two-people.jpg");
    let frame = FrameRef::packed(w, h, PixelFormat::Gray8, &px).unwrap();
    let out = f.step(&frame, Instant::now());
    let target = out.tracks.iter().find(|t| Some(t.id) == out.target).expect("somebody is followed");
    assert!(target.rect.cx() > 0.6, "the nearer, bigger person is on the right: {target:?}");
    assert!(out.ptz.pan >= 0.0, "{:?}", out.ptz);
}

#[test]
fn movenet_puts_the_shoulders_above_the_hips() {
    use godwinmix_track::detect::tract::PoseDetector;
    use godwinmix_track::Joint;
    let Some(boxes) = detector((416, 416)) else { return };
    let mut det = match PoseDetector::new(boxes, 2, None) {
        Ok(d) => d,
        Err(e) => return eprintln!("skipping: no pose model on this machine: {e}"),
    };
    let (px, w, h) = grey("two-people.jpg");
    let frame = FrameRef::packed(w, h, PixelFormat::Gray8, &px).unwrap();
    let start = Instant::now();
    let found = det.detect(&frame);
    eprintln!("boxes and two poses in {:?}", start.elapsed());
    for d in found.iter().take(2) {
        let k = d.keypoints.expect("the best two people have keypoints");
        let shoulders = k.mid(Joint::LeftShoulder, Joint::RightShoulder, 0.3).expect("shoulders seen");
        let hips = k.mid(Joint::LeftHip, Joint::RightHip, 0.3).expect("hips seen");
        assert!(shoulders.y < hips.y, "{shoulders:?} {hips:?}");
        assert!(d.rect.contains(shoulders), "{shoulders:?} outside {:?}", d.rect);
    }
}
