//! A live source that runs behind its own frames is brought back to the newest.
//!
//! The timeline aligner puts a source on the programme's clock when its first
//! segment arrives, and holds that shift for good. A source whose first frames
//! reach the mixer late keeps that lateness for the rest of its life. The
//! camera on Windows is the case that was seen: its sidecar starts capturing,
//! its frames wait in the pipe while the core types the stream and links the
//! decoder, and the first segment lands a second or more after the first frame
//! was taken. Every later frame was then placed that much ahead of when it
//! arrived, so the compositors held each one for that long. On 2026-10-06 the
//! installed app's webcam came up 0.6 to 0.9 s behind and stayed there, and a
//! camera sidecar on a debug core 1.2 s behind.
//!
//! So every frame entering the programme pipeline is measured: how long it will
//! wait before it is due (its lead). Each supervisor tick takes the least lead
//! seen since the last. When every frame for [`TICKS_TO_ACT`] ticks in a row was
//! due more than [`ABOVE`] after it arrived, the source is moved earlier by the
//! least of those, less [`KEEP`]. Frames already queued are then late and the
//! compositors drop them, so the picture jumps to the newest frame. Sound and
//! picture move by the same amount, so lip sync is kept.
//!
//! A source that is not live (a stream that pushes as fast as the queues let
//! it) fills its queues again straight away, to about the lead it had. If a
//! second catch up within [`RELAPSE`] finds three quarters of the first lead
//! again, the guard stops for that source and says so. A live source whose
//! backlog was larger than the programme's queues hold is caught up in two
//! steps, and the second finds less.

use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Instant;

mod aligner;
mod judge;

pub use judge::{Judge, Verdict};
#[cfg(test)]
use judge::{KEEP, RELAPSE};

/// The least lead one branch has seen since the last tick. One atomic minimum
/// per frame and nothing else.
#[derive(Debug)]
pub struct Window(AtomicI64);

impl Default for Window {
    fn default() -> Self {
        Self(AtomicI64::new(i64::MAX))
    }
}

impl Window {
    pub fn note(&self, lead: i64) {
        self.0.fetch_min(lead, Ordering::Relaxed);
    }

    /// The least since the last take, or `None` when no frame came.
    pub fn take(&self) -> Option<i64> {
        let least = self.0.swap(i64::MAX, Ordering::Relaxed);
        (least != i64::MAX).then_some(least)
    }
}

/// What a source's timeline aligner holds for the guard.
#[derive(Default)]
pub struct CatchUp {
    pub video: Window,
    pub audio: Window,
    /// How far this source has been moved earlier since its offset was decided.
    total: AtomicI64,
    /// The src pads of the source's programme queues. The move is their pad
    /// offset: a src pad sends its segment again, shifted, with the next
    /// buffer, so every compositor and mixer pad below it takes the move
    /// without anyone pushing an event.
    pads: Mutex<Vec<gst::Pad>>,
    judge: Mutex<Judge>,
}

impl CatchUp {
    pub fn carry(&self, pad: &gst::Pad) {
        pad.set_offset(-self.total());
        self.pads.lock().push(pad.clone());
    }

    pub fn total(&self) -> i64 {
        self.total.load(Ordering::Relaxed)
    }

    /// Read both windows and decide. Called from the supervisor tick.
    pub fn review(&self, now: Instant) -> (Verdict, Option<i64>) {
        let (v, a) = (self.video.take(), self.audio.take());
        let least = match (v, a) {
            (Some(v), Some(a)) => Some(v.min(a)),
            (v, a) => v.or(a),
        };
        (self.judge.lock().tick(least, now), least)
    }

    /// Move the source `by` nanoseconds earlier. Answers the total moved.
    pub fn apply(&self, by: i64) -> i64 {
        let total = self.total.fetch_add(by, Ordering::Relaxed) + by;
        for pad in self.pads.lock().iter() {
            pad.set_offset(-total);
        }
        total
    }

    /// A restarted or seeked source is placed afresh, so nothing carries over.
    pub fn reset(&self) {
        self.total.store(0, Ordering::Relaxed);
        for pad in self.pads.lock().iter() {
            pad.set_offset(0);
        }
        self.video.take();
        self.audio.take();
        self.judge.lock().reset();
    }
}

#[cfg(test)]
mod tests;
