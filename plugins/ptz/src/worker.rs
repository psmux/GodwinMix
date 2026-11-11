//! One thread per camera that owns its driver. Every call reaches the camera
//! through it, in order, so a slow camera holds up only itself.
//!
//! The joystick is a velocity that lasts `hold` (300 ms by default): each
//! call renews it, and when no call comes in time the thread stops the
//! camera itself. A browser tab closed mid drag, a dropped network or a
//! crashed panel never leaves a camera turning. While the joystick is held
//! the follow loop's speeds are ignored, so the operator always wins.

use std::collections::VecDeque;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use godwinmix_ptz::{Error, Ptz, Velocity};

type Job = Box<dyn FnOnce(&mut dyn Ptz) + Send>;

enum Cmd {
    Joystick(Velocity),
    Steer(Velocity),
    Run(Job),
}

/// What the thread last saw, for `camera_list` and the joystick's answer.
#[derive(Debug, Clone, Default)]
pub struct Status {
    pub moving: Velocity,
    pub by_joystick: bool,
    pub last_error: Option<String>,
    pub watchdog_stops: u64,
}

#[derive(Clone)]
pub struct Handle {
    tx: Sender<Cmd>,
    pub status: Arc<Mutex<Status>>,
}

impl Handle {
    pub fn spawn(id: &str, driver: Box<dyn Ptz>, hold: Duration) -> Handle {
        let (tx, rx) = channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let s = status.clone();
        std::thread::Builder::new().name(format!("ptz-{id}")).spawn(move || run(driver, rx, s, hold)).expect("a camera thread");
        Handle { tx, status }
    }

    /// Move at `v` for the hold time. Answers at once.
    pub fn joystick(&self, v: Velocity) {
        let _ = self.tx.send(Cmd::Joystick(v));
    }

    /// The follow loop's speed. Ignored while the joystick is held.
    pub fn steer(&self, v: Velocity) {
        let _ = self.tx.send(Cmd::Steer(v));
    }

    /// Run `f` on the driver and wait for its answer, at most `wait`.
    pub fn call<T: Send + 'static>(&self, wait: Duration, f: impl FnOnce(&mut dyn Ptz) -> Result<T, Error> + Send + 'static) -> Result<T, String> {
        let (back, answer) = channel();
        let job: Job = Box::new(move |d| {
            let _ = back.send(f(d));
        });
        self.tx.send(Cmd::Run(job)).map_err(|_| "the camera's thread has stopped. Remove the camera and add it again.".to_string())?;
        match answer.recv_timeout(wait) {
            Ok(r) => r.map_err(|e| e.message),
            Err(_) => Err(format!("the camera did not finish within {} ms. It may still be busy with an earlier command; try again.", wait.as_millis())),
        }
    }
}

fn run(mut driver: Box<dyn Ptz>, rx: Receiver<Cmd>, status: Arc<Mutex<Status>>, hold: Duration) {
    let mut held_until: Option<Instant> = None;
    let mut queue: VecDeque<Cmd> = VecDeque::new();
    loop {
        let next = match queue.pop_front() {
            Some(c) => Ok(c),
            None => {
                let wait = held_until.map(|t| t.saturating_duration_since(Instant::now())).unwrap_or(Duration::from_secs(3600));
                rx.recv_timeout(wait)
            }
        };
        match next {
            Ok(Cmd::Run(job)) => job(driver.as_mut()),
            Ok(first) => {
                // Only the newest speed matters; anything else waits its turn.
                let mut latest = first;
                while let Ok(c) = rx.try_recv() {
                    match c {
                        Cmd::Run(_) => queue.push_back(c),
                        speed => latest = speed,
                    }
                }
                let (v, joystick) = match latest {
                    Cmd::Joystick(v) => (v, true),
                    Cmd::Steer(v) => (v, false),
                    Cmd::Run(_) => unreachable!("runs were queued"),
                };
                if !joystick && held_until.is_some() {
                    continue;
                }
                held_until = (joystick && !v.is_stop()).then(|| Instant::now() + hold);
                apply(driver.as_mut(), &status, v, joystick);
            }
            Err(RecvTimeoutError::Timeout) => {
                if held_until.take().is_none() {
                    continue;
                }
                status.lock().unwrap().watchdog_stops += 1;
                apply(driver.as_mut(), &status, Velocity::STOP, true);
            }
            Err(RecvTimeoutError::Disconnected) => {
                let _ = driver.stop();
                return;
            }
        }
    }
}

fn apply(driver: &mut dyn Ptz, status: &Mutex<Status>, v: Velocity, joystick: bool) {
    let out = driver.move_at(v);
    let mut s = status.lock().unwrap();
    s.moving = v;
    s.by_joystick = joystick && !v.is_stop();
    s.last_error = out.err().map(|e| e.message);
}
