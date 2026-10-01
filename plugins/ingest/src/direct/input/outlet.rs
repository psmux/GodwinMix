//! Where every input's tags meet its sink: one timeline across reconnects,
//! and the meter.
//!
//! Each new connection's tags start again at zero. A reader of the stream
//! must never see time go backwards, so each session is laid after the last
//! tag of the one before, a frame later.

use std::sync::{Arc, Mutex, MutexGuard};

use super::super::Sink;
use super::meter::Meter;
use super::stats::InputStats;
use crate::media_tag::MediaTag;
use crate::rtmp::Inlet;

/// The gap left between one session's last tag and the next one's first.
const SESSION_GAP_MS: u32 = 40;

struct Outlet {
    sink: Sink,
    meter: Meter,
    offset_ms: u32,
    /// The latest timestamp handed on, plus one; zero means none yet.
    end_ms: u32,
}

/// Cheap to clone; shared by the streaming threads and the input's own.
#[derive(Clone)]
pub struct Shared(Arc<Mutex<Outlet>>);

impl Shared {
    pub fn new(sink: Sink) -> Shared {
        Shared(Arc::new(Mutex::new(Outlet { sink, meter: Meter::default(), offset_ms: 0, end_ms: 0 })))
    }

    fn lock(&self) -> MutexGuard<'_, Outlet> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Hand one tag on, on the input's timeline.
    pub fn tag(&self, mut tag: MediaTag) {
        let mut o = self.lock();
        tag.timestamp_ms = tag.timestamp_ms.wrapping_add(o.offset_ms);
        o.end_ms = o.end_ms.max(tag.timestamp_ms.wrapping_add(1));
        o.meter.record(&tag);
        o.sink.tag(tag);
    }

    /// A new connection is about to start its timeline at zero.
    pub fn new_session(&self) {
        let mut o = self.lock();
        if o.end_ms > 0 {
            o.offset_ms = o.end_ms + SESSION_GAP_MS;
        }
    }

    /// Fill the meter's numbers into `stats` and hand them to the sink.
    pub fn publish(&self, stats: &mut InputStats) {
        self.publish_with(stats, |_| {});
    }

    /// The same, with `then` writing what the transport knows over the
    /// meter's numbers before they go.
    pub fn publish_with(&self, stats: &mut InputStats, then: impl FnOnce(&mut InputStats)) {
        let mut o = self.lock();
        o.meter.fill(stats);
        then(stats);
        o.sink.stats(stats);
    }

    pub fn quiet_ms(&self) -> Option<u64> {
        self.lock().meter.quiet_ms()
    }

    /// An inlet for the tagger's appsinks, which speak `Inlet`.
    pub fn inlet(&self) -> Box<dyn Inlet> {
        Box::new(ToOutlet(self.clone()))
    }
}

struct ToOutlet(Shared);

impl Inlet for ToOutlet {
    fn tag(&mut self, tag: MediaTag) {
        self.0.tag(tag);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::direct::TagSink;
    use crate::media_tag::TagKind;

    struct Times(Arc<Mutex<Vec<u32>>>);
    impl TagSink for Times {
        fn tag(&mut self, tag: MediaTag) {
            self.0.lock().unwrap().push(tag.timestamp_ms);
        }
        fn stats(&mut self, _: &InputStats) {}
    }

    #[test]
    fn a_second_session_carries_on_after_the_first() {
        let got = Arc::new(Mutex::new(Vec::new()));
        let out = Shared::new(Box::new(Times(got.clone())));
        let at = |ms| MediaTag { kind: TagKind::Video, timestamp_ms: ms, keyframe: true, sequence_header: false, payload: Arc::from(&[0x17u8, 1, 0, 0, 0][..]) };
        out.tag(at(0));
        out.tag(at(1000));
        out.new_session();
        out.tag(at(0));
        assert_eq!(*got.lock().unwrap(), vec![0, 1000, 1041]);
    }
}
