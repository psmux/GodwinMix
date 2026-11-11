//! Choosing who to follow, and framing them.

use std::time::{Duration, Instant};

use godwinmix_track::detect::{Joint, Keypoint, Keypoints};
use godwinmix_track::{
    frame_target, Choose, ChooseConfig, Chooser, Fallback, FollowState, FrameStyle, Framing, Point, Rect,
    Track, TrackId, TrackState,
};

fn track(id: u32, rect: Rect, t: Instant) -> Track {
    Track {
        id: TrackId(id),
        rect,
        velocity: Point::default(),
        score: 0.9,
        class: Default::default(),
        keypoints: None,
        state: TrackState::Confirmed,
        hits: 10,
        first_seen: t,
        last_seen: t,
    }
}

#[test]
fn largest_sticks_until_somebody_is_clearly_bigger() {
    let t = Instant::now();
    let mut c = Chooser::new(Choose::Largest, ChooseConfig::default());
    let a = track(1, Rect::new(0.1, 0.2, 0.1, 0.4), t);
    let mut b = track(2, Rect::new(0.6, 0.2, 0.09, 0.38), t);
    assert_eq!(c.choose(&[a, b], t).target, Some(TrackId(1)));
    b.rect = Rect::new(0.6, 0.2, 0.11, 0.42);
    assert_eq!(c.choose(&[a, b], t).target, Some(TrackId(1)), "a little bigger is not enough");
    b.rect = Rect::new(0.6, 0.1, 0.2, 0.7);
    assert_eq!(c.choose(&[a, b], t).target, Some(TrackId(2)));
}

#[test]
fn a_lost_target_holds_then_falls_back() {
    let t0 = Instant::now();
    let cfg = ChooseConfig { fallback: Fallback::Wide, ..ChooseConfig::default() };
    let hold = cfg.lost_hold;
    let mut c = Chooser::new(Choose::Locked(TrackId(4)), cfg);
    let mut p = track(4, Rect::new(0.4, 0.2, 0.1, 0.4), t0);
    assert_eq!(c.choose(&[p], t0).state, FollowState::Following);
    p.state = TrackState::Lost;
    let other = track(5, Rect::new(0.1, 0.2, 0.1, 0.4), t0);
    let held = c.choose(&[p, other], t0 + Duration::from_millis(100));
    assert_eq!((held.state, held.target, held.rect), (FollowState::Holding, Some(TrackId(4)), Some(p.rect)));
    let gone = c.choose(&[other], t0 + hold + Duration::from_millis(200));
    assert_eq!(gone.state, FollowState::Fallback(Fallback::Wide));
    assert_eq!(gone.target, None, "a locked chooser never picks somebody else");
    p.state = TrackState::Confirmed;
    assert_eq!(c.choose(&[p, other], t0 + hold * 2).target, Some(TrackId(4)), "back when the id is back");
}

#[test]
fn speaker_and_nearest_pick_by_position() {
    let t = Instant::now();
    let tracks = [track(1, Rect::new(0.1, 0.2, 0.1, 0.4), t), track(2, Rect::new(0.7, 0.2, 0.1, 0.4), t)];
    let mut c = Chooser::new(Choose::Speaker { x: 0.8 }, ChooseConfig::default());
    assert_eq!(c.choose(&tracks, t).target, Some(TrackId(2)));
    c.set_policy(Choose::Nearest(Point::new(0.1, 0.5)));
    assert_eq!(c.choose(&tracks, t).target, Some(TrackId(1)));
}

#[test]
fn framings_nest_and_keep_headroom() {
    let t = Instant::now();
    let p = track(1, Rect::new(0.45, 0.2, 0.08, 0.6), t);
    let aspect = 16.0 / 9.0;
    let shot = |f| frame_target(&p, &FrameStyle { framing: f, ..FrameStyle::default() }, aspect);
    let (close, waist, full, wide) = (shot(Framing::Close), shot(Framing::Waist), shot(Framing::Full), shot(Framing::Wide));
    assert!(close.h < waist.h && waist.h < full.h && full.h < wide.h);
    for s in [close, waist, full] {
        let headroom = (p.rect.y - s.y) / s.h;
        assert!((headroom - 0.10).abs() < 0.01, "{s:?}");
        assert!((s.w / s.h - 1.0).abs() < 1e-4, "same aspect as the frame");
        assert!((s.cx() - p.rect.cx()).abs() < 1e-5);
    }
    assert!(full.bottom() > p.rect.bottom(), "the feet are in a full shot");
}

#[test]
fn a_moving_person_gets_lead_room_ahead() {
    let t = Instant::now();
    let mut p = track(1, Rect::new(0.45, 0.2, 0.08, 0.6), t);
    p.velocity = Point::new(0.2, 0.0);
    let s = frame_target(&p, &FrameStyle::default(), 16.0 / 9.0);
    assert!(s.cx() > p.rect.cx() + 0.05, "{s:?}");
}

#[test]
fn keypoints_set_the_waist_line() {
    let t = Instant::now();
    let mut p = track(1, Rect::new(0.4, 0.1, 0.2, 0.8), t);
    let mut k = Keypoints::default();
    let mut put = |j: Joint, y: f32| k.0[j as usize] = Keypoint { at: Point::new(0.5, y), score: 0.9 };
    put(Joint::LeftShoulder, 0.3);
    put(Joint::RightShoulder, 0.3);
    put(Joint::LeftHip, 0.5);
    put(Joint::RightHip, 0.5);
    p.keypoints = Some(k);
    let s = frame_target(&p, &FrameStyle::default(), 16.0 / 9.0);
    // Waist cut at hips plus a tenth of the torso: 0.52.
    assert!((s.bottom() - 0.52).abs() < 1e-4, "{s:?}");
}

#[test]
fn framing_slugs_round_trip() {
    for f in [Framing::Close, Framing::Waist, Framing::Full, Framing::Wide] {
        assert_eq!(Framing::from_slug(f.slug()), Some(f));
    }
}
