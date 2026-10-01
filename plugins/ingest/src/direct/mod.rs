//! The direct host: every show with compositing off, in this one process.
//!
//! ```text
//!   table (configure) ──► Host::apply ──► one Show per row
//!
//!   input (own thread) ──► Switch ──► hub "direct.<id>/main" ──┬──► output (one thread each) ──► RTMP, SRT, UDP, RIST, file
//!   backup (only while needed) ─┘   (main or backup, one         ├──► transcode (once per show) ──► renditions hub ──► outputs
//!                                    unbroken timeline)          ├──► vitals (keyframes only, while asked)
//!                                                                └──► relay, for a show that composites
//! ```
//!
//! A direct show is one input straight to its outputs with no compositor.
//! Its input is demuxed into the same `MediaTag`s a channel stream is made
//! of and published on the hub, and every output reads the hub through a
//! bounded queue of its own, as a channel destination does: a slow output
//! loses GOPs in its own queue and never slows the input or another output.
//! Nothing is decoded unless an output asks for a rendition (`transcode`,
//! once per show however many renditions) or the vitals ask for a picture.
//!
//! The station hands the table over as `direct` in the settings
//! (`dev/plans/wave4-direct-table.md`), and hears back through four events:
//! `direct.input`, `direct.output`, `direct.stats`, and `direct.health`
//! (which the vitals raise). `docs/explanation/direct-host.md` says why it
//! is built this way.

// The inputs (the "directin" work) and the vitals (the "vitals" work) are
// their own modules. Until they merge, a stand in with the same public
// names lets the host build and its tests run: delete the `#[path]` lines
// and the two stand in files when they do.
#[path = "input_standin.rs"]
pub mod input;
#[path = "vitals_standin.rs"]
pub mod vitals;

mod events;
mod host;
mod output;
mod show;
pub mod standalone;
mod table;

#[cfg(test)]
mod tests;

pub use events::Emit;
pub use host::{Host, Relay};

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
