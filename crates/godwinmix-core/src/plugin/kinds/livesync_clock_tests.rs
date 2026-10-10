//! `livesync` in a pipeline whose clock is twenty seconds in, fed a stream
//! stamped from zero: the case of a source restarted twenty seconds into a
//! show.

use super::wrap;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

const FRAME: gst::ClockTime = gst::ClockTime::from_mseconds(40);

/// Run two seconds of 25 fps frames stamped from zero through `livesync`,
/// wrapped or not, on a clock twenty seconds past the base time. Answers the
/// running time of every frame that came out.
fn run(wrapped: bool) -> Option<Vec<gst::ClockTime>> {
    let _ = gst::init();
    let sync = crate::input::optional_livesync("t-vsync").ok()??;
    let caps = gst::Caps::builder("video/x-raw")
        .field("format", "I420")
        .field("width", 64)
        .field("height", 36)
        .field("framerate", gst::Fraction::new(25, 1))
        .build();
    let src = gst_app::AppSrc::builder().caps(&caps).is_live(true).format(gst::Format::Time).build();
    let sink = gst_app::AppSink::builder().sync(false).build();
    let pipeline = gst::Pipeline::new();
    pipeline.add_many([src.upcast_ref(), &sync, sink.upcast_ref()]).unwrap();
    gst::Element::link_many([src.upcast_ref(), &sync, sink.upcast_ref()]).unwrap();
    if wrapped {
        wrap(&src.static_pad("src").unwrap(), &sync, "t");
    }
    let seen = Arc::new(Mutex::new(Vec::new()));
    let into = seen.clone();
    sink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |s| {
                let sample = s.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                let segment = sample.segment().and_then(|s| s.downcast_ref::<gst::ClockTime>().cloned());
                let at = sample.buffer().and_then(|b| b.pts()).zip(segment).and_then(|(p, s)| s.to_running_time(p));
                into.lock().extend(at);
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );
    let clock = gst::SystemClock::obtain();
    pipeline.use_clock(Some(&clock));
    pipeline.set_start_time(gst::ClockTime::NONE);
    pipeline.set_base_time(clock.time() - gst::ClockTime::from_seconds(20));
    pipeline.set_state(gst::State::Playing).unwrap();
    for i in 0..50u64 {
        let mut buffer = gst::Buffer::with_size(64 * 36 * 3 / 2).unwrap();
        buffer.get_mut().unwrap().set_pts(FRAME * i);
        buffer.get_mut().unwrap().set_duration(FRAME);
        if src.push_buffer(buffer).is_err() {
            break;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    pipeline.set_state(gst::State::Null).unwrap();
    let out = seen.lock().clone();
    Some(out)
}

#[test]
fn livesync_hands_back_a_stream_stamped_from_zero_on_its_own_timeline() {
    let Some(out) = run(true) else {
        println!("skipping: livesync is not installed");
        return;
    };
    let last = out.last().copied().unwrap_or_default();
    assert!(out.len() >= 40, "only {} frames came out of two seconds' worth", out.len());
    // Two seconds of frames, give or take the repeats at either end.
    assert!(out.len() <= 70, "{} frames came out of 50: livesync raced to catch the clock", out.len());
    assert!(last < gst::ClockTime::from_seconds(3), "the last frame came out at {last}, on the clock rather than its own timeline");
}

/// The fault itself, kept so that it is seen to be one: without the move
/// `livesync` fills twenty seconds of repeats and stamps them on the clock.
#[test]
fn bare_livesync_puts_a_stream_stamped_from_zero_on_the_clock() {
    let Some(out) = run(false) else { return };
    let last = out.last().copied().unwrap_or_default();
    assert!(last > gst::ClockTime::from_seconds(15), "bare livesync now keeps the stream's own timeline (last {last}); the wrap may be unneeded");
}
