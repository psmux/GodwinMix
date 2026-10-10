//! Where every input's tags meet its sink: one timeline across reconnects,
//! and the meter.
//!
//! Each new connection's tags start again at zero. A reader of the stream
//! must never see time go backwards, so each session is laid after the last
//! tag of the one before, a frame later. A jump inside one session (a
//! sender restarted with its clock somewhere else, which the demuxer did not
//! smooth over) is laid the same way.

use std::sync::{Arc, Mutex, MutexGuard};

use super::super::Sink;
use super::meter::Meter;
use super::stats::InputStats;
use crate::media_tag::MediaTag;
use crate::rtmp::Inlet;

/// The gap left between one session's last tag and the next one's first.
const SESSION_GAP_MS: u32 = 40;
/// Further back than this is a new clock. Audio and video in one transport
/// stream can be a second apart, so this is well beyond that.
const BACK_MS: u32 = 3_000;
/// Further ahead than this is a new clock too: a live feed does not skip ten
/// seconds, and a reader would wait them out.
const AHEAD_MS: u32 = 10_000;

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
        let at = tag.timestamp_ms.wrapping_add(o.offset_ms);
        if o.end_ms > 0 && (at.saturating_add(BACK_MS) < o.end_ms || at > o.end_ms.saturating_add(AHEAD_MS)) {
            eprintln!("DIAG outlet relaid {:?} at={at} end={} offset={}", tag.kind, o.end_ms, o.offset_ms);
            o.offset_ms = o.offset_ms.wrapping_add((o.end_ms + SESSION_GAP_MS).wrapping_sub(at));
        }
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

    #[test]
    fn a_clock_that_jumps_inside_a_session_carries_on_from_where_it_was() {
        let got = Arc::new(Mutex::new(Vec::new()));
        let out = Shared::new(Box::new(Times(got.clone())));
        let at = |ms| MediaTag { kind: TagKind::Video, timestamp_ms: ms, keyframe: true, sequence_header: false, payload: Arc::from(&[0x17u8, 1, 0, 0, 0][..]) };
        for ms in [60_000, 60_040, 59_500, 60_080, 200, 240, 900_000, 900_040] {
            out.tag(at(ms));
        }
        // Half a second back is audio and video apart, and is left alone.
        assert_eq!(*got.lock().unwrap(), vec![60_000, 60_040, 59_500, 60_080, 60_121, 60_161, 60_202, 60_242]);
    }
}
