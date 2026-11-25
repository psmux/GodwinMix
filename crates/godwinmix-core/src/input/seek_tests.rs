use super::*;
use crate::gstutil::make;
use std::time::Duration;

/// A clip with no sound, as the normaliser builds one: the picture into its
/// sink, and an audio chain into a sink of its own with nothing upstream.
fn video_only() -> (gst::Pipeline, gst::Element) {
    let _ = gst::init();
    let pipeline = gst::Pipeline::new();
    let src = make("videotestsrc", "clip").unwrap();
    src.set_property("num-buffers", 300i32);
    let video = make("fakesink", "clip-vproxy").unwrap();
    let aconv = make("audioconvert", "clip-aconv").unwrap();
    let audio = make("fakesink", "clip-aproxy").unwrap();
    // Nothing will ever reach it, so it must not wait to preroll.
    audio.set_property("async", false);
    pipeline.add_many([&src, &video, &aconv, &audio]).unwrap();
    src.link(&video).unwrap();
    aconv.link(&audio).unwrap();
    pipeline.set_state(gst::State::Paused).unwrap();
    let (done, _, _) = pipeline.state(gst::ClockTime::from_seconds(5));
    done.expect("the clip prerolled");
    (pipeline, video)
}

const FLAGS: gst::SeekFlags = gst::SeekFlags::FLUSH.union(gst::SeekFlags::ACCURATE);

/// The cause: the sink with nothing behind it refuses, so the pipeline does.
#[test]
fn a_pipeline_with_a_soundless_audio_chain_refuses_a_seek() {
    let (pipeline, _) = video_only();
    let refused = pipeline.seek_simple(FLAGS, gst::ClockTime::from_seconds(2)).is_err();
    let _ = pipeline.set_state(gst::State::Null);
    assert!(refused, "the pipeline took the seek; the fallback is not needed");
}

/// The fix: the same seek lands through the picture's sink.
#[test]
fn a_video_only_clip_seeks_through_its_picture() {
    let (pipeline, video) = video_only();
    clip(&pipeline, &video, FLAGS, gst::ClockTime::from_seconds(2)).expect("the seek");
    let _ = pipeline.state(gst::ClockTime::from_seconds(5));
    std::thread::sleep(Duration::from_millis(100));
    let at = pipeline.query_position::<gst::ClockTime>();
    let _ = pipeline.set_state(gst::State::Null);
    assert!(at.is_some_and(|t| t >= gst::ClockTime::from_mseconds(1900)), "landed at {at:?}");
}
