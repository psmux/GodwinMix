//! The whole follower in a closed loop: a box sequence for a person walking
//! about a stage, seen through a simulated PTZ camera that turns as the
//! follower tells it. The command stream must be smooth and the camera must
//! end up on the person.

mod common;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use common::{jitter, time};
use godwinmix_track::{Detection, Detector, FollowConfig, FollowOutput, Follower, FrameRef, PixelFormat, Rect};

const FPS: f32 = 25.0;
/// Frame widths a second the camera turns at full pan speed.
const PAN_RATE: f32 = 0.5;

/// Where the person is on the stage, in frame widths from where the camera
/// started: still, walk right, still, walk back further, still.
fn stage_x(n: usize) -> f32 {
    let s = n as f32 / FPS;
    let path = [(0.0, 0.0), (1.0, 0.0), (5.0, 0.8), (8.0, 0.8), (12.0, -0.2), (16.0, -0.2)];
    let w = path.windows(2).find(|w| s < w[1].0).unwrap_or(&path[path.len() - 2..]);
    let f = ((s - w[0].0) / (w[1].0 - w[0].0)).clamp(0.0, 1.0);
    w[0].1 + (w[1].1 - w[0].1) * f
}

/// A camera's eye view of the stage: the person's box minus where the
/// camera points, with a little detector noise.
struct Camera {
    pan: Rc<Cell<f32>>,
    n: usize,
}

impl Detector for Camera {
    fn detect(&mut self, _: &FrameRef) -> Vec<Detection> {
        let n = self.n;
        self.n += 1;
        let x = 0.5 + stage_x(n) - self.pan.get() + jitter(n) * 0.003;
        let r = Rect::from_center(x, 0.5, 0.08, 0.6);
        if r.right() < 0.0 || r.x > 1.0 {
            return Vec::new();
        }
        vec![Detection::new(r.intersection(&Rect::FULL), 0.9)]
    }
}

#[test]
fn the_follower_turns_a_camera_smoothly_onto_a_walking_person() {
    let pan = Rc::new(Cell::new(0.0f32));
    let cfg = FollowConfig::default();
    let (max_speed, max_accel, dead) = (cfg.ptz.max_speed, cfg.ptz.max_accel, cfg.ptz.dead_zone);
    let mut f = Follower::new(Camera { pan: pan.clone(), n: 0 }, cfg);
    let px = [0u8; 16];
    let frame = FrameRef::packed(4, 4, PixelFormat::Gray8, &px).unwrap();
    let (t0, mut out, mut last) = (Instant::now(), FollowOutput::default(), 0.0f32);
    let frames = (16.0 * FPS) as usize;
    let mut starts = 0;
    for n in 0..frames {
        f.step_into(&frame, time(t0, n, FPS), &mut out);
        let v = out.ptz.pan;
        assert!(v.abs() <= max_speed + 1e-6, "frame {n}: {v} over the limit");
        assert!((v - last).abs() <= max_accel / FPS + 1e-4, "frame {n}: {last} to {v} is a jerk");
        assert!(v * last >= 0.0, "frame {n}: turned round without stopping");
        if last == 0.0 && v != 0.0 {
            starts += 1;
        }
        pan.set(pan.get() + v * PAN_RATE / FPS);
        last = v;
    }
    assert_eq!(out.target.map(|t| t.0), Some(1), "one person, one id, all the way");
    assert_eq!(last, 0.0, "the camera is still at the end");
    let off = stage_x(frames) - pan.get();
    assert!(off.abs() < dead, "the person ends {off} from the centre");
    // Two walks, so two moves, and a little settling: not a hunt.
    assert!(starts <= 4, "the camera started {starts} times");
}
