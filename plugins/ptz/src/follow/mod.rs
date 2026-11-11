//! The follow loop: read the camera's picture, run `godwinmix-track`'s
//! Follower on it, and send its pan, tilt and zoom speeds to the camera.
//!
//! One loop per camera, on a thread of its own, and only while asked for. It
//! runs in two modes: following, which steers the camera until `track_stop`,
//! and observing, which only reports what it sees and ends by itself
//! `linger` after the last `track_observe` or `track_state` that asked.

pub mod detector;
mod model;
mod run;

pub use run::follower as run_follower;
pub mod view;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use godwinmix_track::{Choose, FollowOutput, Framing, Point, TrackId};

use crate::frames::Frames;
use detector::Switching;
use crate::worker::Handle;

/// A change to a running loop.
#[derive(Debug, Clone, PartialEq)]
pub enum Ctl {
    /// Start or stop moving the camera.
    Steer(bool),
    Framing(Framing),
    Lock(TrackId),
    Point(Point),
    Policy(Choose),
}

/// How a loop starts: the framing, whether it steers, and the camera's
/// optics for telling its own motion from people's.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    pub framing: Framing,
    pub steering: bool,
    pub optics: Option<godwinmix_track::CameraOptics>,
}

/// What the loop last saw, shared with the tools.
#[derive(Debug, Clone, Default)]
pub struct Seen {
    pub out: FollowOutput,
    pub frames: u64,
    pub last_frame: Option<Instant>,
    pub size: (u32, u32),
    pub error: Option<String>,
    pub from: String,
    pub framing: Option<Framing>,
    pub steering: bool,
    pub ended: bool,
    /// Frames a second over the last few seconds.
    pub fps: f32,
    /// Milliseconds the Follower took on the last frame.
    pub step_ms: f32,
    /// Which detector runs, and why.
    pub detector: detector::Status,
}

pub struct Loop {
    ctl: Sender<Ctl>,
    pub seen: Arc<Mutex<Seen>>,
    stop: Arc<AtomicBool>,
    asked: Arc<Mutex<Instant>>,
}

impl Loop {
    pub fn start(camera: Handle, frames: Box<dyn Frames>, detector: Switching, look: Look, linger: Duration) -> Loop {
        let Look { framing, steering, optics } = look;
        let (ctl, rx) = channel();
        let seen = Arc::new(Mutex::new(Seen { from: frames.describe(), framing: Some(framing), steering, detector: detector.status.clone(), ..Seen::default() }));
        let stop = Arc::new(AtomicBool::new(false));
        let asked = Arc::new(Mutex::new(Instant::now()));
        let job = run::Job { camera, frames, rx, seen: seen.clone(), stop: stop.clone(), asked: asked.clone(), linger, framing, steering, detector: Some(detector), optics };
        std::thread::Builder::new().name("ptz-follow".into()).spawn(move || run::run(job)).expect("a follow thread");
        Loop { ctl, seen, stop, asked }
    }

    pub fn send(&self, c: Ctl) {
        if let Ctl::Steer(on) = c {
            self.seen.lock().unwrap().steering = on;
        }
        let _ = self.ctl.send(c);
    }

    /// Somebody asked what the camera sees: keep an observing loop alive.
    pub fn touch(&self) {
        *self.asked.lock().unwrap() = Instant::now();
    }

    pub fn is_running(&self) -> bool {
        !self.seen.lock().unwrap().ended
    }

    pub fn is_steering(&self) -> bool {
        self.seen.lock().unwrap().steering
    }

    /// End the loop. A steering loop stops the camera on its way out.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    /// Wait until the loop has seen `frames` pictures, or `wait` is up.
    pub fn wait_for(&self, frames: u64, wait: Duration) {
        wait_seen(&self.seen, frames, wait)
    }
}

/// Wait until `seen` counts `frames` pictures, or `wait` is up, without
/// holding anything but `seen` now and then.
pub fn wait_seen(seen: &Mutex<Seen>, frames: u64, wait: Duration) {
    let until = Instant::now() + wait;
    while Instant::now() < until {
        let s = seen.lock().unwrap();
        if s.frames >= frames || s.ended {
            return;
        }
        drop(s);
        std::thread::sleep(Duration::from_millis(20));
    }
}

impl Drop for Loop {
    fn drop(&mut self) {
        self.stop();
    }
}
