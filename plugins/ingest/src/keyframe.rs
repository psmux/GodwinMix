//! Asking a publisher for a keyframe when a reader is waiting for one.
//!
//! A reader of the hub starts at a keyframe, and one that falls behind loses
//! whole GOPs and starts again at the next keyframe. An encoder like OBS sends
//! a keyframe every couple of seconds, so the wait is short. A browser
//! publishing over WebRTC sends one when it starts and then only when the far
//! end asks for it, so a source that was restarted, or a restream that fell
//! behind, waited for good: the source stayed dark with the publisher live.
//!
//! So the video sink asks. A `GstForceKeyUnit` event sent upstream from the
//! sink reaches the RTP session inside `webrtcbin`, which sends the browser a
//! picture loss indication, and the browser answers with a keyframe. Once a
//! second at most while somebody waits; nothing while nobody does. On a
//! pipeline with no RTP session (SRT, a test feed) the event goes up and
//! nothing answers it, which costs nothing.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;

/// How often a waiting reader's request is repeated.
pub const EVERY: Duration = Duration::from_secs(1);

#[derive(Default)]
pub struct Asker {
    last: Mutex<Option<Instant>>,
}

impl Asker {
    /// Ask upstream of `sink` for a keyframe, unless it was asked within [`EVERY`].
    pub fn ask(&self, sink: &gst::Element) {
        if !self.due(Instant::now()) {
            return;
        }
        if let Some(pad) = sink.static_pad("sink") {
            pad.push_event(force_key_unit());
        }
    }

    /// True, and the time noted, when the last ask was long enough ago.
    pub fn due(&self, now: Instant) -> bool {
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        if last.is_some_and(|at| now.duration_since(at) < EVERY) {
            return false;
        }
        *last = Some(now);
        true
    }
}

/// The upstream force key unit event, as `gst_video_event_new_upstream_force_key_unit`
/// makes it: as soon as possible, with the parameter sets.
pub fn force_key_unit() -> gst::Event {
    let s = gst::Structure::builder("GstForceKeyUnit")
        .field("running-time", u64::MAX)
        .field("all-headers", true)
        .field("count", 0u32)
        .build();
    gst::event::CustomUpstream::new(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_waiting_reader_is_asked_for_at_most_once_a_second() {
        let asker = Asker::default();
        let t = Instant::now();
        assert!(asker.due(t));
        assert!(!asker.due(t + Duration::from_millis(500)));
        assert!(asker.due(t + Duration::from_millis(1001)));
    }

    #[test]
    fn the_event_is_the_one_an_rtp_session_turns_into_a_picture_loss_indication() {
        let _ = gst::init();
        let e = force_key_unit();
        assert_eq!(e.type_(), gst::EventType::CustomUpstream);
        let s = e.structure().expect("a structure");
        assert_eq!(s.name(), "GstForceKeyUnit");
        assert_eq!(s.get::<bool>("all-headers").ok(), Some(true));
    }
}
