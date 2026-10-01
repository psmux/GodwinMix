//! The seam the vitals (`vitals/`, owned by its own author) are built on:
//! pictures and the black, freeze and silence checks for one show.
//!
//! ```text
//!   show's hub stream ──► FrameTap (only while asked) ──► decode keyframes ──► FrameSink::video
//!                                                   └──► decode some audio ──► FrameSink::audio
//!   Vitals ──raise / clear──► Alarms ──► the show's health ──► event/direct.health
//! ```
//!
//! Nothing here runs unless the table asks for it: the host starts a
//! show's vitals when its `monitor` has `alarms` or `pictures` on, and
//! drops them when both go off. A tap decodes on its own pipeline's
//! threads and reads the show's stream through a hub reader of its own, so
//! a slow decode loses GOPs in its own queue and never slows the input or
//! an output.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use gstreamer as gst;

use crate::hub::Hub;

/// What the table asks a show to watch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Monitor {
    /// Run the black, freeze and silence checks.
    pub alarms: bool,
    /// Someone is looking: make thumbnails.
    pub pictures: bool,
}

impl Monitor {
    pub fn any(&self) -> bool {
        self.alarms || self.pictures
    }
}

/// One alarm, in the contract's shape.
#[derive(Debug, Clone, PartialEq)]
pub struct Alarm {
    /// `black`, `freeze`, `silence`, or one of the host's own kinds.
    pub kind: String,
    /// Unix ms when it was raised.
    pub since_ms: u64,
    pub detail: String,
}

pub fn unix_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// The alarms one show's vitals have raised. The host reads them into the
/// show's health once a second and says so when the set of kinds changes.
#[derive(Default)]
pub struct Alarms {
    raised: Mutex<Vec<Alarm>>,
}

impl Alarms {
    /// Raise an alarm, or change its detail if it is up. Its age is kept.
    pub fn raise(&self, kind: &str, detail: impl Into<String>) {
        let mut raised = self.raised.lock().unwrap_or_else(|e| e.into_inner());
        match raised.iter_mut().find(|a| a.kind == kind) {
            Some(a) => a.detail = detail.into(),
            None => raised.push(Alarm { kind: kind.into(), since_ms: unix_ms(), detail: detail.into() }),
        }
    }

    pub fn clear(&self, kind: &str) {
        self.raised.lock().unwrap_or_else(|e| e.into_inner()).retain(|a| a.kind != kind);
    }

    pub fn now(&self) -> Vec<Alarm> {
        self.raised.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// What a tap decodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TapWant {
    /// Decode keyframes alone, about one a second, rather than every frame.
    pub keyframes_only: bool,
    /// Decode sound too.
    pub audio: bool,
    /// Of the sound, decode one frame in this many (1 is all of it). An
    /// AAC frame is about 21 ms; 10 keeps a fifth of a second in each two.
    pub audio_every: u32,
}

impl Default for TapWant {
    fn default() -> TapWant {
        TapWant { keyframes_only: true, audio: true, audio_every: 10 }
    }
}

/// Where decoded frames go, called on the tap's streaming threads. Keep it
/// short: the tap's own queue drops GOPs while it is busy.
pub trait FrameSink: Send + Sync {
    /// A decoded picture, raw video in I420 at the stream's own size.
    fn video(&self, sample: &gst::Sample);
    /// Decoded sound, interleaved S16 at the stream's own rate.
    fn audio(&self, sample: &gst::Sample);
}

/// What the host hands one show's vitals.
pub struct Watch {
    pub show: String,
    pub monitor: Monitor,
    pub alarms: Arc<Alarms>,
    pub(crate) hub: Hub,
    pub(crate) app: String,
}

impl Watch {
    /// Start decoding the show's stream into `sink`. Dropping the tap stops
    /// it. A tap made before the input is live waits for it.
    pub fn tap(&self, want: TapWant, sink: Arc<dyn FrameSink>) -> FrameTap {
        super::decode::start(&self.hub, &self.app, want, sink)
    }
}

/// One show's vitals, running. Dropping it stops them.
pub trait Vitals: Send {
    /// The table changed what is asked; never both off (the host drops the
    /// vitals then).
    fn monitor(&mut self, monitor: Monitor);
    /// The newest thumbnail as JPEG, when pictures are on.
    fn thumbnail(&self) -> Option<Arc<[u8]>>;
}

/// Starts one show's vitals. The vitals module provides one; the host takes
/// it in `Host::with_vitals`.
pub type VitalsStart = fn(Watch) -> Box<dyn Vitals>;

/// A running decode of one show's stream.
pub struct FrameTap {
    pub(crate) stop: Arc<AtomicBool>,
}

impl Drop for FrameTap {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
