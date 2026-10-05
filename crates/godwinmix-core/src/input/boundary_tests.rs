use super::boundary::guard_timeline;
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
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
