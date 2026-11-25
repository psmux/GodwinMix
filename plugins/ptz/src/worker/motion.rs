//! The motion lane: the joystick with its watchdog, the follow loop's
//! speeds, and on a one lane camera every other command too.

use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use godwinmix_ptz::{Ptz, Velocity};

use super::{Cmd, Status};

pub(super) fn run(mut driver: Box<dyn Ptz>, rx: Receiver<Cmd>, status: Arc<Mutex<Status>>, hold: Duration) {
    let mut held_until: Option<Instant> = None;
    let mut queue: VecDeque<Cmd> = VecDeque::new();
    loop {
        // A command that ran past the hold time must not leave the camera
        // turning: the watchdog fires before anything else that is queued.
        if held_until.is_some_and(|t| Instant::now() >= t) {
            held_until = None;
            status.lock().unwrap().watchdog_stops += 1;
            apply(driver.as_mut(), &status, Velocity::STOP, true);
        }
        let next = match queue.pop_front() {
            Some(c) => Ok(c),
            None => {
                let wait = held_until.map(|t| t.saturating_duration_since(Instant::now())).unwrap_or(Duration::from_secs(3600));
                rx.recv_timeout(wait)
            }
        };
        match next {
            Ok(Cmd::Run(job)) => job(driver.as_mut()),
            Ok(Cmd::Halt(job)) => {
                // An explicit stop ends the hold too, so no watchdog follows.
                held_until = None;
                job(driver.as_mut());
                let mut s = status.lock().unwrap();
                (s.moving, s.by_joystick) = (Velocity::STOP, false);
            }
            Ok(first) => {
                // Only the newest speed matters; anything else waits its turn.
                let mut latest = first;
                while let Ok(c) = rx.try_recv() {
                    match c {
                        Cmd::Run(_) | Cmd::Halt(_) => queue.push_back(c),
                        speed => latest = speed,
                    }
                }
                let (v, joystick) = match latest {
                    Cmd::Joystick(v) => (v, true),
                    Cmd::Steer(v) => (v, false),
                    Cmd::Run(_) | Cmd::Halt(_) => unreachable!("runs were queued"),
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
