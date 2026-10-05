//! The loop on the open's own thread: try, wait, try again.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{lock, Stage};
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

    pub(super) fn set(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// Try, wait, try again, until the open works or the source is stopped.
pub(super) fn keep_opening<F>(stage: &Mutex<Stage>, cancel: &Cancel, mut open: F)
where
    F: FnMut(&Cancel) -> Result<Capture, String>,
{
    let mut wait = RETRY_FIRST;
    while !cancel.is_set() {
        let result = open(cancel);
        let mut now = lock(stage);
        // Stopped while the open ran: nobody is going to ask for it now, and
        // dropping it on the way out hands the device back.
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
