//! The motion detector on a camera that pans: told how the camera moves, it
//! sees nothing while the picture slides and learns the new view once the
//! camera is still; not told, it reads the sliding background as motion.

use godwinmix_track::{Detection, Detector, EgoMotion, FrameRef, MotionDetector, PixelFormat, Rect};

const W: usize = 160;
const H: usize = 90;

/// A stage wider than the picture with a texture that never repeats, a
/// person in a striped shirt at `person` (stage pixels), seen from `pan`.
fn paint(pan: f32, person: f32, out: &mut Vec<u8>) {
    out.clear();
    for y in 0..H {
        for x in 0..W {
            let sx = x as f32 + pan;
            let v = 80.0 + 40.0 * ((sx * 0.21).sin() * (y as f32 * 0.17 + sx * 0.05).cos());
            let inside = (sx - person).abs() < 7.0 && (25..85).contains(&y);
            let v = if inside { if ((sx - person) as i32).rem_euclid(4) < 2 { 230.0 } else { 30.0 } } else { v };
            out.push(v as u8);
        }
    }
}

fn detect(d: &mut impl Detector, pan: f32, person: f32, px: &mut Vec<u8>) -> Vec<Detection> {
    paint(pan, person, px);
    d.detect(&FrameRef::packed(W as u32, H as u32, PixelFormat::Gray8, px).unwrap())
}

/// Detections that are not the person, whose box is centred `at` pixels
/// across the picture: nowhere near them, or far bigger than them (the
/// whole sliding picture read as one moving thing).
fn strays(dets: &[Detection], at: f32) -> usize {
    let person = Rect::new(at / W as f32 - 0.08, 0.2, 0.16, 0.75);
    dets.iter().filter(|d| d.rect.intersection(&person).area() <= 0.0 || d.rect.area() > 2.0 * person.area()).count()
}

/// Still, then a pan of 2 pixels a frame (0.3 frames a second at 25 fps),
/// then still again; the person walks a pixel a frame throughout. Returns
/// the stray detections during the pan and the detections after it.
fn run(told: bool, step: f32) -> (usize, Vec<Vec<Detection>>) {
    let mut d = MotionDetector::default();
    let (mut px, mut pan, mut person) = (Vec::new(), 0.0f32, 60.0f32);
    for _ in 0..20 {
        person += 1.0;
        detect(&mut d, pan, person, &mut px);
    }
    let pan_ego = EgoMotion { dx: -step * 25.0 / W as f32, dy: 0.0, zoom: 0.0 };
    let mut stray = 0;
    for _ in 0..25 {
        if told {
            d.camera_motion(pan_ego);
        }
        (pan, person) = (pan + step, person + 1.0);
        let dets = detect(&mut d, pan, person, &mut px);
        stray += if told { dets.len() } else { strays(&dets, person - pan) };
    }
    if told {
        d.camera_motion(EgoMotion::STILL);
    }
    let after = (0..10)
        .map(|_| {
            person += 1.0;
            detect(&mut d, pan, person, &mut px)
        })
        .collect();
    (stray, after)
}

#[test]
fn told_the_camera_pans_it_sees_nothing_then_learns_the_new_view() {
    let (during, after) = run(true, 2.0);
    assert_eq!(during, 0, "nothing is detected while the camera pans");
    assert!(after[0].is_empty(), "the first still frame is learned as the new background");
    let person = 60.0 + 20.0 + 25.0;
    let found = after.iter().enumerate().skip(2).filter(|(i, d)| !d.is_empty() && strays(d, person + 1.0 + *i as f32 - 50.0) == 0).count();
    assert!(found >= 6, "the walking person is found again, and only them: {after:?}");
}

#[test]
fn not_told_it_reads_the_pan_as_motion() {
    for step in [0.5, 1.0, 2.0] {
        let (stray, _) = run(false, step);
        assert!(stray > 10, "at {step} pixels a frame the sliding background reads as motion in most frames: {stray} of 25");
    }
}
