//! The steering on a recorded box sequence: a simulated PTZ camera follows a
//! person who walks and stops, and the command stream must be smooth.

mod common;

use std::time::Instant;

use common::{jitter, time};
use godwinmix_track::{CropConfig, CropSteer, PtzConfig, PtzSteer, Rect};

const FPS: f32 = 25.0;

/// Where the person stands in the world, in frame widths from where the
/// camera started: walk right for four seconds, stand, walk back, stand.
fn person_x(n: usize) -> f32 {
    let s = n as f32 / FPS;
    let x = match s {
        s if s < 1.0 => 0.0,
        s if s < 5.0 => (s - 1.0) * 0.15,
        s if s < 8.0 => 0.6,
        s if s < 10.0 => 0.6 - (s - 8.0) * 0.2,
        _ => 0.2,
    };
    x + jitter(n) * 0.004
}

#[test]
fn a_simulated_camera_follows_smoothly_and_settles() {
    let cfg = PtzConfig::default();
    let (mut steer, t0) = (PtzSteer::new(cfg), Instant::now());
    // The camera turns at most 0.6 frame widths a second at full speed.
    let (mut cam, mut last, mut flips_in_dead_zone) = (0.0f32, 0.0f32, 0);
    let frames = (14.0 * FPS) as usize;
    for n in 0..frames {
        let in_view = 0.5 + person_x(n) - cam;
        let target = Rect::from_center(in_view, 0.5, 0.3, 0.6);
        let v = steer.step(Some(target), time(t0, n, FPS));
        assert!(v.pan.abs() <= cfg.max_speed + 1e-6, "frame {n}: {v:?} over the speed limit");
        let accel = (v.pan - last).abs() * FPS;
        assert!(accel <= cfg.max_accel + 1e-3, "frame {n}: speed jumped {last} to {}", v.pan);
        assert!(v.pan * last >= 0.0, "frame {n}: turned round without stopping, {last} to {}", v.pan);
        if (in_view - 0.5).abs() < cfg.dead_zone && last == 0.0 && v.pan != 0.0 {
            flips_in_dead_zone += 1;
        }
        assert_eq!(v.tilt, 0.0, "frame {n}: the person never moved up or down");
        cam += v.pan * 0.6 / FPS;
        last = v.pan;
    }
    assert_eq!(flips_in_dead_zone, 0, "the camera started moving while the person was in the dead zone");
    assert_eq!(last, 0.0, "the camera is still at the end");
    let off = (0.5 + person_x(frames) - cam - 0.5).abs();
    assert!(off < cfg.dead_zone, "the person ends {off} from the centre");
}

#[test]
fn a_person_swaying_inside_the_dead_zone_never_moves_the_camera() {
    let (mut steer, t0) = (PtzSteer::new(PtzConfig::default()), Instant::now());
    for n in 0..200 {
        let x = 0.5 + 0.04 * (n as f32 * 0.3).sin();
        let v = steer.step(Some(Rect::from_center(x, 0.5, 0.6, 0.6)), time(t0, n, FPS));
        assert_eq!(v.pan, 0.0, "frame {n}");
    }
}

#[test]
fn losing_the_target_eases_to_a_stop() {
    let cfg = PtzConfig::default();
    let (mut steer, t0) = (PtzSteer::new(cfg), Instant::now());
    let mut v = Default::default();
    for n in 0..25 {
        v = steer.step(Some(Rect::from_center(0.95, 0.5, 0.2, 0.4)), time(t0, n, FPS));
    }
    assert!(v.pan > 0.5 && v.zoom > 0.0, "{v:?}");
    let mut prev = v.pan;
    for n in 25..60 {
        let v = steer.step(None, time(t0, n, FPS));
        assert!(v.pan <= prev && v.pan >= 0.0 && prev - v.pan <= cfg.max_accel / FPS + 1e-6);
        prev = v.pan;
    }
    assert_eq!(prev, 0.0);
}

#[test]
fn the_crop_eases_to_its_target_without_overshoot() {
    let (mut crop, t0) = (CropSteer::new(CropConfig::default()), Instant::now());
    let goal = Rect::from_center(0.7, 0.4, 0.3, 0.3);
    let mut prev = crop.step(None, t0);
    for n in 1..100 {
        let r = crop.step(Some(goal), time(t0, n, FPS));
        assert!(r.cx() >= prev.cx() - 1e-6 && r.cx() <= goal.cx() + 1e-6, "frame {n}: {r:?}");
        assert!(r.x >= 0.0 && r.right() <= 1.0 + 1e-6 && r.y >= 0.0 && r.bottom() <= 1.0 + 1e-6);
        prev = r;
    }
    assert!((prev.cx() - 0.7).abs() < 0.01 && (prev.h - 0.3).abs() < 0.01, "{prev:?}");
}
