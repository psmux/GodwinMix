//! Raw video carries no decode time.
//!
//! A queue measures what it holds by each buffer's decode time when it has
//! one and by its presentation time only when it does not
//! (`GST_BUFFER_DTS_OR_PTS`). A raw frame has no decode order, but a live
//! source with `do-timestamp` stamps both times, and `videorate` copies the
//! decode time of the frame it repeats onto every repeat while it counts the
//! presentation time on. So when a source falls behind and `videorate` fills
//! the gap, every repeated frame says it was decoded at the same instant: each
//! queue below reads its level as zero, its time limit never binds, and it
//! fills to its byte backstop instead, twice the frames it was sized for.
//!
//! The 0.3.25 test on 2026-11-02 watched exactly that. The HTML newsroom set,
//! an `appsrc` with `do-timestamp`, fell behind under a cutout scene and four
//! queues on its path each held 60 frames of 1080p, 186 MB, at a level of
//! 0 ms: `set-newsroom-vprog-q`, `pgm-vq-set-newsroom@N` and two slot queues.
//!
//! [`present_only`] clears the decode time on every buffer leaving a pad, so
//! everything below it is measured by when it is shown. The buffer is made
//! writable for that, which for a buffer nobody else holds costs nothing and
//! for a shared one copies its header, never its picture.

use gstreamer as gst;

/// Clear the decode time of every buffer that leaves `pad`.
pub fn present_only(pad: &gst::Pad) {
    pad.add_probe(gst::PadProbeType::BUFFER | gst::PadProbeType::BUFFER_LIST, |_, info| {
        match &mut info.data {
            Some(gst::PadProbeData::Buffer(buffer)) => clear(buffer),
            Some(gst::PadProbeData::BufferList(list)) => {
                list.make_mut().foreach_mut(|mut buffer, _| {
                    clear(&mut buffer);
                    std::ops::ControlFlow::Continue(Some(buffer))
                });
            }
            _ => {}
        }
        gst::PadProbeReturn::Ok
    });
}

fn clear(buffer: &mut gst::Buffer) {
    if buffer.dts().is_some() {
        buffer.make_mut().set_dts(gst::ClockTime::NONE);
    }
}

#[cfg(test)]
#[path = "decode_time_tests.rs"]
mod tests;
