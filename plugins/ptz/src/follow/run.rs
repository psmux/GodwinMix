//! The loop's thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use godwinmix_ptz::Velocity;
use godwinmix_track::{Detector, Fallback, FollowConfig, FollowOutput, FollowState, Follower, FrameStyle, Framing, MotionConfig, MotionDetector};

use super::{Ctl, Seen};
use crate::frames::Frames;
use crate::worker::Handle;

pub struct Job {
    pub camera: Handle,
    pub frames: Box<dyn Frames>,
    pub rx: Receiver<Ctl>,
    pub seen: Arc<Mutex<Seen>>,
    pub stop: Arc<AtomicBool>,
    pub asked: Arc<Mutex<Instant>>,
    pub linger: Duration,
    pub framing: Framing,
    pub steering: bool,
}

/// The detector the loop runs. Motion for now: no model to fetch, and it
/// runs on a Raspberry Pi. The tracking engine's ONNX detector replaces it
/// when its `tract` feature lands in this plugin.
pub fn detector() -> Box<dyn Detector + Send> {
    Box::new(MotionDetector::new(MotionConfig::default()))
}

pub fn follower(framing: Framing) -> Follower {
    let style = FrameStyle { framing, ..FrameStyle::default() };
    Follower::new(detector(), FollowConfig { style, ..FollowConfig::default() })
}

fn apply(f: &mut Follower, c: Ctl, steering: &mut bool) {
    match c {
        Ctl::Steer(on) => {
            *steering = on;
            if on {
                let policy = f.config().policy;
                f.set_policy(policy);
            } else {
                f.stop();
            }
        }
        Ctl::Framing(fr) => f.set_framing(fr),
        Ctl::Lock(id) => f.follow(id),
        Ctl::Point(p) => f.follow_point(p),
        Ctl::Policy(p) => f.set_policy(p),
    }
}

pub fn run(mut job: Job) {
    let mut f = follower(job.framing);
    let mut out = FollowOutput::default();
    let mut steering = job.steering;
    let mut in_fallback = false;
    let mut fps_window: (Instant, u64) = (Instant::now(), 0);
    while !job.stop.load(Ordering::SeqCst) {
        if !steering && job.asked.lock().unwrap().elapsed() > job.linger {
            break;
        }
        let was = steering;
        while let Ok(c) = job.rx.try_recv() {
            apply(&mut f, c, &mut steering);
        }
        if was && !steering {
            job.camera.steer(Velocity::STOP);
        }
        let picture = match job.frames.next(Duration::from_millis(500)) {
            Ok(Some(p)) => p,
            Ok(None) => continue,
            Err(why) => {
                job.seen.lock().unwrap().error = Some(why);
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
        };
        let Some(frame) = picture.frame() else { continue };
        let started = Instant::now();
        f.step_into(&frame, started, &mut out);
        let step_ms = started.elapsed().as_secs_f32() * 1000.0;
        if steering {
            job.camera.steer(Velocity::new(out.ptz.pan, out.ptz.tilt, out.ptz.zoom));
            in_fallback = fallback(&job.camera, &out.state, in_fallback, &mut f);
        }
        let mut s = job.seen.lock().unwrap();
        s.frames += 1;
        fps_window.1 += 1;
        if fps_window.0.elapsed() >= Duration::from_secs(2) {
            s.fps = fps_window.1 as f32 / fps_window.0.elapsed().as_secs_f32();
            fps_window = (Instant::now(), 0);
        }
        (s.last_frame, s.size, s.error, s.step_ms) = (Some(Instant::now()), (picture.width, picture.height), None, step_ms);
        s.framing = Some(f.config().style.framing);
        s.out.clone_from(&out);
    }
    if steering {
        job.camera.steer(Velocity::STOP);
    }
    let mut s = job.seen.lock().unwrap();
    (s.ended, s.steering) = (true, false);
}

/// On the way into a preset fallback, go to that preset once and start the
/// tracker afresh on the new picture.
fn fallback(camera: &Handle, state: &FollowState, was: bool, f: &mut Follower) -> bool {
    let FollowState::Fallback(Fallback::Preset(name)) = state else { return false };
    if !was {
        if let Ok(n) = name.trim_start_matches("preset-").parse::<u16>() {
            let _ = camera.call(Duration::from_secs(2), move |d| d.goto_preset(n));
            f.reset();
        }
    }
    true
}
