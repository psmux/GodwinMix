//! The threads that own one camera's drivers. Every call reaches the camera
//! through them, so a slow camera holds up only itself.
//!
//! Two lanes per camera when the protocol can take two connections at once
//! (`Ptz::second`: ONVIF, Canon and the fake): the motion lane carries the
//! joystick, the follow loop's speeds, `stop` and `zoom`; the command lane
//! carries everything slow (presets, position, focus, exposure, goto). A
//! preset that takes the camera two seconds to store never holds a move
//! back. VISCA keeps one lane, because the camera keeps one sequence count
//! and many take one TCP link only; there a move waits for at most the one
//! command in flight, and speeds always go before queued commands.
//!
//! The joystick is a velocity that lasts `hold` (300 ms by default): each
//! call renews it, and when no call comes in time the motion lane stops the
//! camera itself. A browser tab closed mid drag, a dropped network or a
//! crashed panel never leaves a camera turning. While the joystick is held
//! the follow loop's speeds are ignored, so the operator always wins.

mod motion;

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use godwinmix_ptz::{Error, Ptz, Velocity};

pub(crate) type Job = Box<dyn FnOnce(&mut dyn Ptz) + Send>;

pub(crate) enum Cmd {
    Joystick(Velocity),
    Steer(Velocity),
    Run(Job),
    /// A stop: runs like `Run`, and ends the joystick's hold.
    Halt(Job),
}

/// What the motion lane last saw, for `camera_list` and the joystick's answer.
#[derive(Debug, Clone, Default)]
pub struct Status {
    pub moving: Velocity,
    pub by_joystick: bool,
    pub last_error: Option<String>,
    pub watchdog_stops: u64,
}

#[derive(Clone)]
pub struct Handle {
    motion: Sender<Cmd>,
    commands: Sender<Cmd>,
    pub status: Arc<Mutex<Status>>,
    /// 2 when moves and slow commands have a connection each, 1 otherwise.
    pub lanes: u8,
}

impl Handle {
    pub fn spawn(id: &str, driver: Box<dyn Ptz>, hold: Duration) -> Handle {
        let second = driver.second();
        let (motion, rx) = channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let s = status.clone();
        std::thread::Builder::new().name(format!("ptz-{id}")).spawn(move || motion::run(driver, rx, s, hold)).expect("a camera thread");
        let (commands, lanes) = match second {
            Some(d) => {
                let (tx, rx) = channel();
                std::thread::Builder::new().name(format!("ptz-{id}-cmd")).spawn(move || serve(d, rx)).expect("a camera thread");
                (tx, 2)
            }
            None => (motion.clone(), 1),
        };
        Handle { motion, commands, status, lanes }
    }

    /// Move at `v` for the hold time. Answers at once.
    pub fn joystick(&self, v: Velocity) {
        let _ = self.motion.send(Cmd::Joystick(v));
    }

    /// The follow loop's speed. Ignored while the joystick is held.
    pub fn steer(&self, v: Velocity) {
        let _ = self.motion.send(Cmd::Steer(v));
    }

    /// Stop the camera on the motion lane and end the joystick's hold. Waits
    /// for the camera's answer, at most `wait`, behind no slow command.
    pub fn halt(&self, wait: Duration) -> Result<(), String> {
        call_on(&self.motion, Cmd::Halt, wait, |d| d.stop())
    }

    /// Run `f` on the motion lane (`zoom`) and wait for its answer, at most
    /// `wait`. Never queued behind a slow command.
    pub fn motion_call<T: Send + 'static>(&self, wait: Duration, f: impl FnOnce(&mut dyn Ptz) -> Result<T, Error> + Send + 'static) -> Result<T, String> {
        call_on(&self.motion, Cmd::Run, wait, f)
    }

    /// Run `f` on the command lane and wait for its answer, at most `wait`.
    pub fn call<T: Send + 'static>(&self, wait: Duration, f: impl FnOnce(&mut dyn Ptz) -> Result<T, Error> + Send + 'static) -> Result<T, String> {
        call_on(&self.commands, Cmd::Run, wait, f)
    }
}

fn call_on<T: Send + 'static>(lane: &Sender<Cmd>, kind: fn(Job) -> Cmd, wait: Duration, f: impl FnOnce(&mut dyn Ptz) -> Result<T, Error> + Send + 'static) -> Result<T, String> {
    let (back, answer) = channel();
    let job: Job = Box::new(move |d| {
        let _ = back.send(f(d));
    });
    lane.send(kind(job)).map_err(|_| "the camera's thread has stopped. Remove the camera and add it again.".to_string())?;
    match answer.recv_timeout(wait) {
        Ok(r) => r.map_err(|e| e.message),
        Err(_) => Err(format!("the camera did not finish within {} ms. It may still be busy with an earlier command; try again.", wait.as_millis())),
    }
}

/// The command lane: one command at a time, in order, on its own driver.
fn serve(mut driver: Box<dyn Ptz>, rx: Receiver<Cmd>) {
    while let Ok(cmd) = rx.recv() {
        if let Cmd::Run(job) | Cmd::Halt(job) = cmd {
            job(driver.as_mut());
        }
    }
}
