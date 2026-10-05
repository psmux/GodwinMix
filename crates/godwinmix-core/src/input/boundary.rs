//! The last check on a source's timing before its media crosses to the
//! programme.
//!
//! Every compositor and audio mixer in the programme works in running time,
//! and a buffer it cannot place there is not an error it reports. It is a
//! `g_assert` in `gst_video_aggregator_fill_queues`, which aborts the whole
//! process, every source and every output with it. So whatever a source's own
//! pipeline does (a demuxer that hands on a byte segment, a parser that lets a
//! buffer out before any segment, a decoder that loses its timestamps), it
//! stops here, on the proxy sink's sink pad, and costs that source alone.
//!
//! A segment in any format but time is dropped, the source's pipeline posts an
//! error, and the mixer restarts it the way it restarts any source that
//! failed. Until a time segment arrives again nothing of that stream crosses.
//! A buffer with no segment in front of it, or with no timestamp, is dropped
//! on its own: the stream is otherwise sound, and the next good buffer goes
//! through.

use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::warn;

/// What one guard remembers between buffers.
struct Guard {
    source: String,
    media: &'static str,
    /// A segment that was not in time arrived and no time segment since.
    broken: AtomicBool,
    /// A buffer has been dropped for having no timestamp, so the warning is
    /// said once per stream rather than once a frame.
    untimed: AtomicBool,
}

/// Guard the sink pad of `proxy`, the proxy sink a source's `media` crosses
/// to the programme by.
pub fn guard_timeline(proxy: &gst::Element, source: &str, media: &'static str) -> Result<()> {
    let pad = proxy
        .static_pad("sink")
        .with_context(|| format!("{} has no sink pad", proxy.name()))?;
    let guard = Arc::new(Guard {
        source: source.to_string(),
        media,
        broken: AtomicBool::new(false),
        untimed: AtomicBool::new(false),
    });
    let element = proxy.downgrade();
    let kinds = gst::PadProbeType::EVENT_DOWNSTREAM
        | gst::PadProbeType::BUFFER
        | gst::PadProbeType::BUFFER_LIST;
    pad.add_probe(kinds, move |pad, info| match &info.data {
        Some(gst::PadProbeData::Event(e)) => guard.event(e, element.upgrade().as_ref()),
        Some(gst::PadProbeData::Buffer(b)) => guard.buffer(pad, b.pts()),
        Some(gst::PadProbeData::BufferList(l)) => guard.buffer(pad, l.get(0).and_then(|b| b.pts())),
        _ => gst::PadProbeReturn::Ok,
    })
    .context("installing the timeline guard")?;
    Ok(())
}

impl Guard {
    fn event(&self, event: &gst::Event, proxy: Option<&gst::Element>) -> gst::PadProbeReturn {
        let gst::EventView::Segment(segment) = event.view() else {
            return gst::PadProbeReturn::Ok;
        };
        let format = segment.segment().format();
        if format == gst::Format::Time {
            self.broken.store(false, Ordering::Relaxed);
            self.untimed.store(false, Ordering::Relaxed);
            return gst::PadProbeReturn::Ok;
        }
        if !self.broken.swap(true, Ordering::Relaxed) {
            warn!(
                source = %self.source, media = self.media, ?format,
                "a source's media arrived in a segment that is not in time; it was held back \
                 from the programme and the source is being restarted"
            );
            if let Some(proxy) = proxy {
                proxy.post_error_message(gst::error_msg!(
                    gst::StreamError::Format,
                    (
                        "the {} of {} came in a {:?} segment rather than a time segment, so it was \
                         held back from the programme and the source is restarting. If this keeps \
                         happening, the source's container or decoder is at fault: read its log",
                        self.media,
                        self.source,
                        format
                    )
                ));
            }
        }
        gst::PadProbeReturn::Drop
    }

    fn buffer(&self, pad: &gst::Pad, pts: Option<gst::ClockTime>) -> gst::PadProbeReturn {
        if self.broken.load(Ordering::Relaxed) {
            return gst::PadProbeReturn::Drop;
        }
        let timed = pad
            .sticky_event::<gst::event::Segment>(0)
            .is_some_and(|s| s.segment().format() == gst::Format::Time);
        if timed && pts.is_some() {
            return gst::PadProbeReturn::Ok;
        }
        if !self.untimed.swap(true, Ordering::Relaxed) {
            warn!(
                source = %self.source, media = self.media, segment = timed,
                "a source's buffer had no segment or no timestamp in front of it; it was dropped \
                 at the programme's edge, and so will any more like it"
            );
        }
        gst::PadProbeReturn::Drop
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use std::time::Duration;

    /// An appsrc into a guarded proxy sink, with a counter on what got past.
    fn guarded() -> (gst::Pipeline, gstreamer_app::AppSrc, Arc<AtomicU64>, gst::Bus) {
        gst::init().unwrap();
        let pipeline = gst::Pipeline::new();
        let src = gstreamer_app::AppSrc::builder().format(gst::Format::Time).build();
        let sink = gst::ElementFactory::make("fakesink").property("sync", false).build().unwrap();
        pipeline.add_many([src.upcast_ref(), &sink]).unwrap();
        src.link(&sink).unwrap();
        guard_timeline(&sink, "odd-camera", "video").unwrap();
        let passed = Arc::new(AtomicU64::new(0));
        let counted = passed.clone();
        sink.static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, _| {
            counted.fetch_add(1, Ordering::Relaxed);
            gst::PadProbeReturn::Ok
        });
        pipeline.set_state(gst::State::Playing).unwrap();
        let bus = pipeline.bus().unwrap();
        (pipeline, src, passed, bus)
    }

    fn push(src: &gstreamer_app::AppSrc, ms: Option<u64>) {
        let mut buffer = gst::Buffer::with_size(16).unwrap();
        buffer.get_mut().unwrap().set_pts(ms.map(gst::ClockTime::from_mseconds));
        let _ = src.push_buffer(buffer);
    }

    fn settle(passed: &AtomicU64, want: u64) -> u64 {
        let until = std::time::Instant::now() + Duration::from_secs(2);
        while passed.load(Ordering::Relaxed) < want && std::time::Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(100));
        passed.load(Ordering::Relaxed)
    }

    /// A byte segment stops the stream and asks for a restart; a time segment
    /// after it lets the stream through again.
    #[test]
    fn a_segment_not_in_time_is_held_back_and_the_source_asks_to_restart() {
        let (pipeline, src, passed, bus) = guarded();
        push(&src, Some(0));
        let first = settle(&passed, 1);
        let entry = src.static_pad("src").unwrap();
        let bytes = gst::FormattedSegment::<gst::format::Bytes>::new();
        entry.push_event(gst::event::Segment::new(&bytes));
        push(&src, Some(40));
        push(&src, Some(80));
        let held = settle(&passed, first + 1);
        let error = bus.timed_pop_filtered(gst::ClockTime::from_seconds(2), &[gst::MessageType::Error]);
        let time = gst::FormattedSegment::<gst::ClockTime>::new();
        entry.push_event(gst::event::Segment::new(&time));
        push(&src, Some(120));
        let after = settle(&passed, held + 1);
        pipeline.set_state(gst::State::Null).unwrap();
        assert_eq!(first, 1, "the first buffer did not get through");
        assert_eq!(held, first, "a buffer behind a byte segment crossed to the programme");
        let error = error.expect("the source did not ask to be restarted");
        let gst::MessageView::Error(e) = error.view() else { unreachable!() };
        assert!(e.error().to_string().contains("odd-camera"), "{}", e.error());
        assert_eq!(after, held + 1, "the stream did not come back after a time segment");
    }

    /// A buffer with no timestamp is dropped alone and the stream goes on.
    #[test]
    fn a_buffer_with_no_timestamp_is_dropped_and_the_next_one_passes() {
        let (pipeline, src, passed, _bus) = guarded();
        push(&src, Some(0));
        let first = settle(&passed, 1);
        push(&src, None);
        push(&src, Some(80));
        let after = settle(&passed, first + 1);
        pipeline.set_state(gst::State::Null).unwrap();
        assert_eq!(after, first + 1, "the untimed buffer passed or the next one did not");
    }
}
