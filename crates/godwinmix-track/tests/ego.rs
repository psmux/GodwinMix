//! The follower on a panning camera, in a closed loop: three people on a
//! stage, one of them walking past another and back, seen through a
//! simulated PTZ camera that turns as the follower tells it. With the
//! camera's optics the tracker knows its own pan from the people's walking:
//! ids hold (also for a person the pan takes out of view and brings back),
//! people standing still have no velocity, and the camera does not hunt.

mod common;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use common::{jitter, time};
use godwinmix_track::{CameraOptics, Detection, Detector, FollowConfig, FollowOutput, Follower, FrameRef, FrameStyle, Framing, PixelFormat, Rect};

const FPS: f32 = 25.0;
/// Frame widths a second the camera turns at full pan speed, and the log of
/// how much the picture grows in a second at full zoom speed.
const PAN_RATE: f32 = 0.5;
const ZOOM_RATE: f32 = 0.7;
const SECONDS: f32 = 13.0;

/// Stage x, in frame widths from where the camera starts, of the walker:
/// still, right past the person at 0.3, still, back to the left, still.
fn walker_x(n: usize) -> f32 {
    let s = n as f32 / FPS;
    let path = [(0.0, 0.0), (1.0, 0.0), (3.5, 0.55), (5.5, 0.55), (8.5, -0.05), (SECONDS, -0.05)];
    let w = path.windows(2).find(|w| s < w[1].0).unwrap_or(&path[path.len() - 2..]);
    let f = ((s - w[0].0) / (w[1].0 - w[0].0)).clamp(0.0, 1.0);
    w[0].1 + (w[1].1 - w[0].1) * f
}

/// Stage x and box height of each person: the walker is nearest, so tallest.
fn people(n: usize) -> [(f32, f32); 3] {
    [(-0.12, 0.5), (walker_x(n), 0.66), (0.3, 0.52)]
}

/// The camera's eye view: every person's box minus where the camera
/// points, magnified by its zoom about the centre of the picture.
struct Camera {
    pan: Rc<Cell<f32>>,
    zoom: Rc<Cell<f32>>,
    n: usize,
}

impl Detector for Camera {
    fn detect(&mut self, _: &FrameRef) -> Vec<Detection> {
        let n = self.n;
        self.n += 1;
        let mut out = Vec::new();
        for (i, (x, h)) in people(n).into_iter().enumerate() {
            let z = self.zoom.get();
            let cx = 0.5 + (x - self.pan.get()) * z + jitter(n * 3 + i) * 0.003;
            let r = Rect::from_center(cx, 0.5 + 0.05 * z, h * 0.14 * z, h * z);
            if r.right() > 0.02 && r.x < 0.98 {
                out.push(Detection::new(r.intersection(&Rect::FULL), 0.9));
            }
        }
        out
    }
}

struct Run {
    ids: u32,
    targets: Vec<u32>,
    worst_still_velocity: f32,
    starts: u32,
    end_off: f32,
    last: f32,
}

fn run(optics: Option<CameraOptics>) -> Run {
    let (pan, zoom) = (Rc::new(Cell::new(0.0f32)), Rc::new(Cell::new(1.0f32)));
    let style = FrameStyle { framing: Framing::Full, ..FrameStyle::default() };
    let cfg = FollowConfig { camera: optics, style, ..FollowConfig::default() };
    let (max_speed, max_accel) = (cfg.ptz.max_speed, cfg.ptz.max_accel);
    let mut f = Follower::new(Camera { pan: pan.clone(), zoom: zoom.clone(), n: 0 }, cfg);
    let px = [0u8; 16];
    let frame = FrameRef::packed(4, 4, PixelFormat::Gray8, &px).unwrap();
    let (t0, mut out, mut last) = (Instant::now(), FollowOutput::default(), 0.0f32);
    let mut r = Run { ids: 0, targets: Vec::new(), worst_still_velocity: 0.0, starts: 0, end_off: 0.0, last: 0.0 };
    let frames = (SECONDS * FPS) as usize;
    for n in 0..frames {
        f.step_into(&frame, time(t0, n, FPS), &mut out);
        let v = out.ptz.pan;
        assert!(v.abs() <= max_speed + 1e-6, "frame {n}: {v} over the limit");
        assert!((v - last).abs() <= max_accel / FPS + 1e-4, "frame {n}: {last} to {v} is a jerk");
        assert!(v * last >= 0.0, "frame {n}: turned round without stopping");
        r.starts += u32::from(last == 0.0 && v != 0.0);
        r.ids = r.ids.max(out.tracks.iter().map(|t| t.id.0).max().unwrap_or(0));
        if let Some(t) = out.target.filter(|t| r.targets.last() != Some(&t.0)) {
            r.targets.push(t.0);
        }
        // The two people standing still, while the camera turns fast and
        // the walker is not passing them.
        if v.abs() > 0.3 {
            for (x, _) in [people(n)[0], people(n)[2]].into_iter().filter(|(x, _)| (x - walker_x(n)).abs() > 0.1) {
                let at = 0.5 + (x - pan.get()) * zoom.get();
                // In full view: a box the frame's edge cuts moves as it is cut.
                let whole = |t: &&godwinmix_track::Track| t.rect.x > 0.01 && t.rect.right() < 0.99;
                let near = out.tracks.iter().filter(|t| t.is_confirmed() && (t.rect.cx() - at).abs() < 0.04).filter(whole);
                for t in near {
                    r.worst_still_velocity = r.worst_still_velocity.max(t.velocity.x.abs());
                }
            }
        }
        // The pan is in stage units, as degrees are: zoomed in, the same
        // turn moves the picture further.
        pan.set(pan.get() + v * PAN_RATE / FPS);
        // A camera's zoom stops at its ends, whatever it is told.
        zoom.set((zoom.get() * (out.ptz.zoom * ZOOM_RATE / FPS).exp()).clamp(1.0, 4.0));
        last = v;
    }
    (r.end_off, r.last) = ((walker_x(frames) - pan.get()) * zoom.get(), last);
    r
}

/// Optics that match the simulated camera: full pan speed turns it half a
/// frame a second, full zoom speed grows the picture by e^0.7 a second.
fn matched() -> CameraOptics {
    CameraOptics { hfov_deg: 60.0, pan_deg_s: 60.0 * PAN_RATE, zoom_rate: ZOOM_RATE, ..CameraOptics::default() }
}

#[test]
fn on_a_panning_camera_ids_hold_still_people_stay_still_and_the_camera_does_not_hunt() {
    let r = run(Some(matched()));
    assert_eq!(r.targets.len(), 1, "one target all the way, not a swap: {:?}", r.targets);
    assert_eq!(r.ids, 3, "three people, three ids, including the one the pan took out of view and back");
    // What is left is a person coming back in at the frame's edge, whose
    // box grows as they come in; it fades within half a second.
    assert!(r.worst_still_velocity < 0.08, "people standing still read as moving at {} frames a second while the camera pans", r.worst_still_velocity);
    assert!(r.starts <= 4, "two walks, so two moves and a little settling, not a hunt: {} starts", r.starts);
    assert_eq!(r.last, 0.0, "the camera is still at the end");
    assert!(r.end_off.abs() < FollowConfig::default().ptz.dead_zone, "the walker ends {} from the centre", r.end_off);
}

#[test]
fn without_the_optics_the_pan_reads_as_everyone_walking() {
    let blind = run(None);
    let seen = run(Some(matched()));
    assert!(blind.worst_still_velocity > 0.15, "the pan shows as velocity on people standing still: {}", blind.worst_still_velocity);
    assert!(blind.worst_still_velocity > 3.0 * seen.worst_still_velocity, "the optics take most of it out: {} blind, {} with", blind.worst_still_velocity, seen.worst_still_velocity);
}
