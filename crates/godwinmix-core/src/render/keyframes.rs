//! Keyframes at the same running times on every encoder of a ladder.
//!
//! Each encoder's own interval starts counting when that encoder starts, so
//! two rungs started a second apart would never put a keyframe on the same
//! frame, and an ABR player switching between them would wait. Instead every
//! encoder of a source watches the timestamps going in and, on the first
//! frame at or past each multiple of the plan's interval, asks for a
//! keyframe. Every rung reads the same programme frames, so every rung asks
//! on the same frame.
//!
//! The probe runs on the encoder's streaming thread and does one division
//! and, once an interval, pushes one event. Nothing else.

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video::DownstreamForceKeyUnitEvent;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// The interval index a timestamp falls in.
pub fn slot(pts_ns: u64, interval_ms: u32) -> u64 {
    pts_ns / (u64::from(interval_ms.max(1)) * 1_000_000)
}

/// Ask `encoder` for a keyframe at every multiple of `interval_ms` of
/// running time.
pub fn align(encoder: &gst::Element, interval_ms: u32) {
    let Some(sink) = encoder.static_pad("sink") else { return };
    let last = Arc::new(AtomicU64::new(u64::MAX));
    sink.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
        let Some(gst::PadProbeData::Buffer(buf)) = &info.data else {
            return gst::PadProbeReturn::Ok;
        };
        let Some(pts) = buf.pts() else { return gst::PadProbeReturn::Ok };
        // Running time, which every rung shares whatever its segment says.
        let at = running_time(pad, pts).unwrap_or(pts);
        let n = slot(at.nseconds(), interval_ms);
        if last.swap(n, Ordering::Relaxed) == n {
            return gst::PadProbeReturn::Ok;
        }
        // Pushed from the peer, on this thread, so the encoder handles it
        // before the buffer the probe is holding.
        if let Some(peer) = pad.peer() {
            // At this frame's own running time, which is the frame the
            // probe is holding. Without one, the encoder takes the event
            // for a repeat of the last request and ignores it.
            let ev = DownstreamForceKeyUnitEvent::builder()
                .timestamp(pts)
                .running_time(at)
                .all_headers(true)
                .count(n as u32)
                .build();
            peer.push_event(ev);
        }
        gst::PadProbeReturn::Ok
    });
}

/// A timestamp on `pad` as running time, from the segment it arrived in.
pub fn running_time(pad: &gst::Pad, pts: gst::ClockTime) -> Option<gst::ClockTime> {
    let ev = pad.sticky_event::<gst::event::Segment>(0)?;
    let seg = ev.segment().downcast_ref::<gst::ClockTime>()?.clone();
    seg.to_running_time(pts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_timestamp_in_one_interval_has_one_index() {
        let ms = 1_000_000;
        assert_eq!(slot(0, 2000), 0);
        assert_eq!(slot(1999 * ms, 2000), 0);
        assert_eq!(slot(2000 * ms, 2000), 1);
        assert_eq!(slot(4100 * ms, 2000), 2);
    }
}
