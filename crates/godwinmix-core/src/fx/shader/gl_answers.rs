//! What has come back from the GPU, and stopping the GL pipeline.

use super::{frames, Gl};
use crate::overlay::blend::Planes;
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::Ordering;

impl Gl {
    /// Whether an answer has come back yet.
    pub fn answered(&self) -> bool {
        self.latest.lock().is_some()
    }

    /// Draw the newest answer onto `f` and send nothing, for a caller that
    /// waited for the answer to the one frame it sent.
    pub fn draw_latest(&self, f: &mut Planes<'_>) {
        if let Some(answer) = self.latest.lock().clone() {
            frames::draw(&self.stacked, &answer, f);
        }
    }

    /// Drop every answer so far, for a caller about to wait for a new one.
    pub fn forget(&self) {
        while self.sink.try_pull_sample(gst::ClockTime::ZERO).is_some() {}
        *self.latest.lock() = None;
    }

    /// Take in what has come back, without drawing it.
    pub fn answered_now(&self) -> bool {
        while let Some(s) = self.sink.try_pull_sample(gst::ClockTime::ZERO) {
            *self.latest.lock() = s.buffer_owned();
            self.waiting.store(0, Ordering::Relaxed);
        }
        self.answered()
    }

    pub fn has_failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }
}

impl Gl {
    /// Stop the pipeline now, on this thread, for a caller that may wait:
    /// the GL probe and the preview strip, which run on a worker. A process
    /// that ends right after must not have a GL context still going down on
    /// a thread of its own.
    pub fn close(self) {
        self.closed.store(true, Ordering::Release);
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

impl Drop for Gl {
    fn drop(&mut self) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        let pipeline = self.pipeline.clone();
        let _ = std::thread::Builder::new().name("gmx-fx-gl-stop".into()).spawn(move || {
            let _ = pipeline.set_state(gst::State::Null);
        });
    }
}
