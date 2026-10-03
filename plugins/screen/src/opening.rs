//! Opening the capture off the call that asked for it.
//!
//! The protocol asks `start` to return as soon as the producer is running and
//! never to wait for the first frame, and the core stops waiting after five
//! seconds. On a Windows laptop with an Intel Arc GPU, loading GStreamer's
//! Direct3D 11 plugin alone took five seconds, because it sets up every
//! adapter before the capture element exists, and the first frame came two
//! to four seconds after that. So a start after a stall ran out of time, the
//! core gave up on it, and the screen stayed on "connecting".
//!
//! Here the open runs on a thread of its own and `start` answers at once. The
//! picture follows when the capture is up, and `health` says what is
//! happening until then.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use godwinmix_capture_common::Capture;
use godwinmix_sdk::prelude::*;

/// Where an open stands.
enum Stage {
    Opening,
    Open(Capture),
    Failed(String),
}

/// One open, from the moment it is asked for until it is let go of.
pub struct Opening {
    stage: Arc<Mutex<Stage>>,
    cancelled: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Opening {
    /// Run `open` on a thread and answer straight away.
    pub fn start<F>(open: F) -> Opening
    where
        F: FnOnce() -> Result<Capture, String> + Send + 'static,
    {
        let stage = Arc::new(Mutex::new(Stage::Opening));
        let cancelled = Arc::new(AtomicBool::new(false));
        let (into, gone) = (Arc::clone(&stage), Arc::clone(&cancelled));
        let thread = std::thread::Builder::new()
            .name("gmx-screen-open".into())
            .spawn(move || {
                let result = open();
                // Let go of here, not stored, if the source was stopped while
                // the open ran: nobody is going to ask for it now.
                if gone.load(Ordering::SeqCst) {
                    return;
                }
                *lock(&into) = match result {
                    Ok(capture) => Stage::Open(capture),
                    Err(why) => Stage::Failed(why),
                };
            })
            .ok();
        Opening { stage, cancelled, thread }
    }

    /// What `health` says: the capture's own answer once it is open.
    pub fn health(&self, what: &str) -> Health {
        match &*lock(&self.stage) {
            Stage::Opening => Health::degraded(format!("{what} is opening")),
            Stage::Open(capture) => capture.health(what),
            Stage::Failed(why) => Health::failing(format!(
                "{what} would not start: {why}. Restart the source to try again."
            )),
        }
    }

    /// Stop, without waiting for an open that is still running.
    pub fn stop(mut self, drain: std::time::Duration) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Stage::Open(capture) = std::mem::replace(&mut *lock(&self.stage), Stage::Opening) {
            capture.drain(drain);
        }
        // Detached: the thread ends when its open does, and drops what it
        // made because `cancelled` is set.
        self.thread.take();
    }
}

fn lock(stage: &Mutex<Stage>) -> std::sync::MutexGuard<'_, Stage> {
    stage.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn a_slow_open_answers_at_once_and_says_it_is_opening() {
        let asked = Instant::now();
        let opening = Opening::start(|| {
            std::thread::sleep(Duration::from_millis(300));
            Err("the display went away".into())
        });
        assert!(asked.elapsed() < Duration::from_millis(100), "start waited for the open");
        let health = opening.health("the screen");
        assert_eq!(health.state, HealthState::Degraded, "{health:?}");
        std::thread::sleep(Duration::from_millis(600));
        let health = opening.health("the screen");
        assert_eq!(health.state, HealthState::Failing, "{health:?}");
        assert!(health.detail.unwrap_or_default().contains("the display went away"));
    }

    #[test]
    fn stopping_while_it_opens_does_not_wait_for_it() {
        let opening = Opening::start(|| {
            std::thread::sleep(Duration::from_millis(500));
            Err("too late".into())
        });
        let asked = Instant::now();
        opening.stop(Duration::from_millis(10));
        assert!(asked.elapsed() < Duration::from_millis(100));
    }
}
