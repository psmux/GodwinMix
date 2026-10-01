//! The direct host: shows with no compositor, one input straight to its
//! outputs. This file is a stand in written by the inputs work until the
//! host's own lands; it holds only the three names an input is written
//! against, in the shape the host was asked to give them.

pub mod input;

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use crate::media_tag::MediaTag;
pub use input::InputStats;

/// Where an input's tags and numbers go. Called from GStreamer streaming
/// threads, so nothing in it may wait for long.
pub trait TagSink: Send {
    fn tag(&mut self, tag: MediaTag);
    /// The input's numbers, about once a second.
    fn stats(&mut self, stats: &InputStats);
}

pub type Sink = Box<dyn TagSink>;

/// Set once to make an input's `run` return. Cheap to clone.
#[derive(Clone, Default)]
pub struct StopSignal(Arc<(Mutex<bool>, Condvar)>);

impl StopSignal {
    pub fn stop(&self) {
        *self.0 .0.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.0 .1.notify_all();
    }

    pub fn is_stopped(&self) -> bool {
        *self.0 .0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Wait up to `wait`, waking early on a stop. True when stopped.
    pub fn wait(&self, wait: Duration) -> bool {
        let guard = self.0 .0.lock().unwrap_or_else(|e| e.into_inner());
        let (guard, _) = self.0 .1.wait_timeout_while(guard, wait, |stopped| !*stopped).unwrap_or_else(|e| e.into_inner());
        *guard
    }
}

/// One input. `run` blocks its thread until `stop` is set, reconnecting
/// through loss and failure on its own.
pub trait Input: Send {
    fn run(self: Box<Self>, out: Sink, stop: StopSignal);
}
