//! The tracker on synthetic scenes: people who move, cross and hide.

mod common;

use std::time::Instant;

use common::{detections, paint, time, Walker};
use godwinmix_track::{Detection, FrameRef, PixelFormat, Rect, TrackId, TrackState, Tracker};

const FPS: f32 = 30.0;

#[test]
fn two_people_who_cross_keep_their_ids() {
    let walkers = [
        Walker::new(Rect::new(0.1, 0.3, 0.1, 0.4), 0.25, [200, 30, 30]),
        Walker::new(Rect::new(0.8, 0.32, 0.1, 0.4), -0.25, [30, 30, 200]),
    ];
    let (mut tr, t0, mut dets) = (Tracker::default(), Instant::now(), Vec::new());
    let mut first = Vec::new();
    for n in 0..90 {
        detections(&walkers, n, FPS, &mut dets);
        let tracks = tr.update(&dets, time(t0, n, FPS));
        if n == 0 {
            first = tracks.iter().map(|t| (t.id, t.rect.cx())).collect();
            assert_eq!(first.len(), 2, "both people are tracked from the first frame");
        }
    }
    // After crossing, the one who started on the left is on the right and
    // moving right, under the same id.
    let left_id = first.iter().min_by(|a, b| a.1.total_cmp(&b.1)).unwrap().0;
    let t = tr.get(left_id).expect("the left person is still tracked");
    assert!(t.rect.cx() > 0.7 && t.velocity.x > 0.15, "{t:?}");
    assert_eq!(tr.tracks().len(), 2);
}

#[test]
fn a_person_hidden_behind_the_pulpit_comes_back_with_the_same_id() {
    // Hidden for two seconds, out again a little further on: too far for IoU.
    let w = Walker::new(Rect::new(0.2, 0.3, 0.08, 0.4), 0.1, [40, 160, 40]).hide(30, 90);
    let other = Walker::new(Rect::new(0.75, 0.3, 0.08, 0.4), 0.0, [200, 200, 30]);
    let walkers = [w, other];
    let (mut tr, t0, mut dets, mut px) = (Tracker::default(), Instant::now(), Vec::new(), Vec::new());
    let mut id = None;
    for n in 0..120 {
        detections(&walkers, n, FPS, &mut dets);
        paint(&walkers, n, FPS, 320, 180, &mut px);
        let f = FrameRef::packed(320, 180, PixelFormat::Rgb8, &px).unwrap();
        let tracks = tr.update_with_frame(&dets, Some(&f), time(t0, n, FPS));
        let green = tracks.iter().find(|t| t.rect.cx() < 0.72);
        match n {
            0 => id = green.map(|t| t.id),
            60 => assert_eq!(green.map(|t| t.state), Some(TrackState::Lost)),
            119 => assert_eq!(green.map(|t| (t.id, t.state)), Some((id.unwrap(), TrackState::Confirmed))),
            _ => {}
        }
    }
    assert!(tr.tracks().iter().all(|t| t.id.0 <= 2), "no new id was made: {:?}", tr.tracks());
}

#[test]
fn a_single_false_detection_never_becomes_a_track() {
    let w = [Walker::new(Rect::new(0.4, 0.3, 0.1, 0.4), 0.0, [0; 3])];
    let (mut tr, t0, mut dets) = (Tracker::default(), Instant::now(), Vec::new());
    for n in 0..30 {
        detections(&w, n, FPS, &mut dets);
        if n == 10 {
            dets.push(Detection::new(Rect::new(0.85, 0.1, 0.05, 0.1), 0.95));
        }
        tr.update(&dets, time(t0, n, FPS));
    }
    assert_eq!(tr.tracks().len(), 1);
    assert_eq!(tr.tracks()[0].id, TrackId(1));
}

#[test]
fn a_weak_detection_keeps_a_track_alive_but_cannot_start_one() {
    let (mut tr, t0) = (Tracker::default(), Instant::now());
    let r = Rect::new(0.4, 0.3, 0.1, 0.4);
    tr.update(&[Detection::new(r, 0.9)], t0);
    for n in 1..20 {
        let dets = [Detection::new(r, 0.3), Detection::new(Rect::new(0.0, 0.0, 0.1, 0.1), 0.3)];
        let tracks = tr.update(&dets, time(t0, n, FPS));
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].state, TrackState::Confirmed);
    }
}

#[test]
fn a_lost_track_is_forgotten_after_max_lost() {
    let (mut tr, t0) = (Tracker::default(), Instant::now());
    tr.update(&[Detection::new(Rect::new(0.4, 0.3, 0.1, 0.4), 0.9)], t0);
    let max = tr.config().max_lost.as_secs_f32();
    tr.update(&[], time(t0, 1, FPS));
    assert_eq!(tr.tracks()[0].state, TrackState::Lost);
    tr.update(&[], time(t0, ((max + 0.5) * FPS) as usize, FPS));
    assert!(tr.tracks().is_empty());
}

#[test]
fn coasting_moves_tracks_without_losing_them() {
    let w = [Walker::new(Rect::new(0.2, 0.3, 0.1, 0.4), 0.3, [0; 3])];
    let (mut tr, t0, mut dets) = (Tracker::default(), Instant::now(), Vec::new());
    for n in 0..30 {
        detections(&w, n, FPS, &mut dets);
        tr.update(&dets, time(t0, n, FPS));
    }
    let before = tr.tracks()[0].rect.cx();
    let after = tr.coast(time(t0, 35, FPS))[0];
    assert_eq!(after.state, TrackState::Confirmed);
    assert!((after.rect.cx() - before - 0.3 * 6.0 / FPS).abs() < 0.01, "{before} then {}", after.rect.cx());
}

#[test]
fn keypoints_ride_along_with_the_box_until_fresh_ones_come() {
    use godwinmix_track::{Keypoint, Keypoints, Point};
    let (mut tr, t0) = (Tracker::default(), Instant::now());
    let r = Rect::new(0.4, 0.3, 0.1, 0.4);
    let mut k = Keypoints::default();
    k.0[0] = Keypoint { at: Point::new(0.45, 0.35), score: 0.9 };
    tr.update(&[Detection { keypoints: Some(k), ..Detection::new(r, 0.9) }], t0);
    let moved = Rect::new(0.42, 0.3, 0.1, 0.4);
    let t = tr.update(&[Detection::new(moved, 0.9)], time(t0, 1, FPS))[0];
    let nose = t.keypoints.expect("kept").0[0].at;
    assert!((nose.x - 0.47).abs() < 1e-4 && (nose.y - 0.35).abs() < 1e-4, "{nose:?}");
}
