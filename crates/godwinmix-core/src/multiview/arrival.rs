//! The programme tile, stamped with when it reaches the mosaic.
//!
//! The programme's mixers run with a second of upstream latency
//! (`MIN_UPSTREAM_LATENCY_NS` in mixer.rs). While every input has a frame
//! ready they do not wait for it, and a programme frame leaves `vmix` a few
//! tens of milliseconds after its running time. When one input stops
//! delivering (a camera that stalls, a browser whose decoder refuses its
//! frames) they wait out the whole second for every frame. The programme is
//! still right, because its own sinks were told about that second, but every
//! frame now reaches the mosaic most of a second after its running time, and
//! the mosaic, which keeps a quarter of a second of slack and shares the
//! programme's clock, skips each one as too late. The programme tile then
//! holds its last frame for as long as the input stays stalled, which looks to
//! an operator like the whole mixer has stopped.
//!
//! So the programme tile's frames are stamped with the running time at which
//! they arrive, instead of the running time they were made for. A frame is
//! then never late for the mosaic, the tile shows the freshest programme there
//! is, and when the mixers are not waiting nothing changes: arrival is within
//! a frame of the stamp the frame already had. The source tiles keep their own
//! stamps, so a tile that has stopped still shows stopped.
//!
//! The stamp used to be a tenth of a second before arrival. That made each
//! programme frame end a tenth of a second sooner than the mosaic frame it was
//! needed for, so the mosaic waited for the next one before it drew anything,
//! and every tile on it, not only the programme's, came out that much later:
//! on 2026-10-06, a source tile 175 ms behind the wall clock with the lag and
//! 105 ms without it, once the tiles lost their second `videorate` too.

use gstreamer as gst;
use gstreamer::prelude::*;

/// Restamp every buffer leaving `pad` with the running time it left at, in
/// the pad's own segment.
pub fn stamp_on_arrival(pad: &gst::Pad) {
    pad.add_probe(gst::PadProbeType::BUFFER, |pad, info| {
        let Some(now) = running_time_now(pad) else { return gst::PadProbeReturn::Ok };
        let Some(segment) = pad.sticky_event::<gst::event::Segment>(0) else { return gst::PadProbeReturn::Ok };
        let Ok(segment) = segment.segment().clone().downcast::<gst::ClockTime>() else {
            return gst::PadProbeReturn::Ok;
        };
        let Some(pts) = restamp(&segment, now) else { return gst::PadProbeReturn::Ok };
        if let Some(buffer) = info.buffer_mut() {
            let buffer = buffer.make_mut();
            buffer.set_pts(pts);
            buffer.set_dts(gst::ClockTime::NONE);
        }
        gst::PadProbeReturn::Ok
    });
}

/// The running time of the pipeline `pad` is in, from its clock and base time.
fn running_time_now(pad: &gst::Pad) -> Option<gst::ClockTime> {
    let element = pad.parent_element()?;
    let clock = element.clock()?;
    let base = element.base_time()?;
    clock.time().checked_sub(base)
}

/// The stream time in `segment` for a frame that arrived at running time `now`.
pub fn restamp(segment: &gst::FormattedSegment<gst::ClockTime>, now: gst::ClockTime) -> Option<gst::ClockTime> {
    segment.position_from_running_time(now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_is_stamped_when_it_arrived() {
        let _ = gst::init();
        let segment = gst::FormattedSegment::<gst::ClockTime>::new();
        let now = gst::ClockTime::from_seconds(31);
        assert_eq!(restamp(&segment, now), Some(now));
    }

    #[test]
    fn a_segment_that_starts_later_is_respected() {
        let _ = gst::init();
        let mut segment = gst::FormattedSegment::<gst::ClockTime>::new();
        segment.set_start(gst::ClockTime::from_seconds(100));
        segment.set_time(gst::ClockTime::from_seconds(100));
        // Running time 5 s is stream position 105 s in a segment that starts at 100.
        assert_eq!(restamp(&segment, gst::ClockTime::from_seconds(5)), Some(gst::ClockTime::from_seconds(105)));
    }
}
