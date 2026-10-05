//! Opening a capture off the call that asked for it.
//!
//! The protocol asks `start` to return as soon as the producer is running and
//! never to wait for the first frame, and the core stops waiting after five
//! seconds. Opening a device is often slower than that on Windows. Loading
//! GStreamer's Direct3D 11 plugin took five seconds on a laptop with an Intel
//! Arc GPU before the screen capture element even existed. A USB webcam took
//! four seconds for the device monitor to list it and three to seven more for
//! Media Foundation to hand over the first frame. Each of those ran past the
//! core's five seconds, the core gave up on the source, and it never came up.
//!
//! So the open runs on a thread of its own and `start` answers at once. The
//! picture follows when the device is up, and `health` says what is happening
//! until then. An open that fails is tried again on the same thread, waiting a
//! little longer each time, so a camera another app was holding comes up by
//! itself once that app lets go. Nobody has to restart the source.
//!
//! Two opens never race for one device. A new open waits for the thread of the
//! one it replaces to finish before it asks for anything, and an open that
//! finishes after it was stopped lets go of what it opened straight away.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use godwinmix_sdk::wire::Health;

use crate::Capture;

/// How long to wait before the first retry of a failed open. Doubles with each
/// failure up to [`RETRY_MAX`].
pub const RETRY_FIRST: Duration = Duration::from_secs(1);
/// The longest wait between two tries. Long enough that a camera unplugged for
/// the night costs one device scan every half minute, short enough that one
/// plugged back in is up before anyone goes looking.
pub const RETRY_MAX: Duration = Duration::from_secs(30);

/// Whether the open has been told to stop. Handed to the open so it can give
/// up between attempts rather than open a device nobody wants.
#[derive(Clone, Debug, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn is_set(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Where an open stands.
enum Stage {
    Opening(Instant),
    Open(Capture),
    Failed { why: String, next: Instant },
    Stopped,
}

/// One open, from the moment it is asked for until it is let go of.
pub struct Opening {
    stage: Arc<Mutex<Stage>>,
    cancel: Cancel,
    thread: Option<JoinHandle<()>>,
}

impl Opening {
    /// Run `open` on a thread and answer straight away.
    pub fn start<F>(open: F) -> Opening
    where
        F: FnMut(&Cancel) -> Result<Capture, String> + Send + 'static,
    {
        Opening::after(None, open)
    }

    /// The same, once the thread of `before` has finished, so the device it
    /// held is free before this one asks for it. `before` is stopped first.
    pub fn after<F>(before: Option<Opening>, open: F) -> Opening
    where
        F: FnMut(&Cancel) -> Result<Capture, String> + Send + 'static,
    {
        let behind = before.and_then(Opening::stop_detached);
        let stage = Arc::new(Mutex::new(Stage::Opening(Instant::now())));
        let cancel = Cancel::default();
        let (into, told) = (Arc::clone(&stage), cancel.clone());
        let thread = std::thread::Builder::new()
            .name("gmx-capture-open".into())
            .spawn(move || {
                if let Some(behind) = behind {
                    let _ = behind.join();
                }
                keep_opening(&into, &told, open);
            })
            .ok();
        Opening {
            stage,
            cancel,
            thread,
        }
    }

    /// What `health` says: opening, then the capture's own answer once it is
    /// open, or why it failed and when it tries again.
    pub fn health(&self, what: &str) -> Health {
        match &*lock(&self.stage) {
            Stage::Opening(since) => Health::degraded(format!(
                "{what} is opening ({:.1} s so far). The picture follows when it is up.",
                since.elapsed().as_secs_f32()
            )),
            Stage::Open(capture) => capture.health(what),
            Stage::Failed { why, next } => {
                let left = next.saturating_duration_since(Instant::now());
                let when = if left.is_zero() {
                    "Trying again now.".to_string()
                } else {
                    format!("The next try is in {} s.", left.as_secs().max(1))
                };
                Health::failing(format!("{what} would not start: {why}. {when}"))
            }
            Stage::Stopped => Health::ok(),
        }
    }

    /// Whether the capture is open now.
    pub fn is_open(&self) -> bool {
        matches!(&*lock(&self.stage), Stage::Open(_))
    }

    /// Stop. A capture that is open is drained for at most `drain` and let go
    /// of here; an open still running is told to stop and lets go of what it
    /// made when it finishes. Nothing here waits for a device to answer.
    pub fn stop(self, drain: Duration) {
        let was = std::mem::replace(&mut *lock(&self.stage), Stage::Stopped);
        if let Stage::Open(capture) = was {
            capture.drain(drain);
        }
        self.stop_detached();
    }

    /// Tell the thread to stop and hand back its handle, for an open that
    /// must not start until it has finished.
    fn stop_detached(mut self) -> Option<JoinHandle<()>> {
        self.let_go();
        self.thread.take()
    }

    fn let_go(&self) {
        self.cancel.0.store(true, Ordering::SeqCst);
        // Dropped here, which takes the pipeline to NULL.
        drop(std::mem::replace(&mut *lock(&self.stage), Stage::Stopped));
        if let Some(thread) = &self.thread {
            thread.thread().unpark();
        }
    }
}

/// An open let go of without `stop` still stops: a thread left retrying
/// would open the device again for nobody.
impl Drop for Opening {
    fn drop(&mut self) {
        self.let_go();
    }
}

/// Try, wait, try again, until the open works or the source is stopped.
fn keep_opening<F>(stage: &Mutex<Stage>, cancel: &Cancel, mut open: F)
where
    F: FnMut(&Cancel) -> Result<Capture, String>,
{
    let mut wait = RETRY_FIRST;
    while !cancel.is_set() {
        let result = open(cancel);
        let mut now = lock(stage);
        // Stopped while the open ran: nobody is going to ask for it now, and
        // dropping it here hands the device back.
        if cancel.is_set() {
            *now = Stage::Stopped;
            return;
        }
        match result {
            Ok(capture) => {
                *now = Stage::Open(capture);
                return;
            }
            Err(why) => {
                *now = Stage::Failed {
                    why,
                    next: Instant::now() + wait,
                }
            }
        }
        drop(now);
        let until = Instant::now() + wait;
        while !cancel.is_set() && Instant::now() < until {
            std::thread::park_timeout(until.saturating_duration_since(Instant::now()));
        }
        wait = (wait * 2).min(RETRY_MAX);
    }
}

fn lock(stage: &Mutex<Stage>) -> MutexGuard<'_, Stage> {
    stage.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests;
