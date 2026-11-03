//! The zero time queue fill of 0.3.25, reproduced with the real elements: a
//! source stamped the way `do-timestamp` stamps it, a gap, `videorate`
//! filling it at the canvas rate, and a slot sized queue whose consumer has
//! stopped, as a programme that has fallen behind stops.

use super::*;
use crate::gstutil::backstop::{by_backstop, bytes_for, held_by_backstop};
use crate::gstutil::{capsfilter, make, queue_thread};
use gstreamer::prelude::*;
use gstreamer_app::AppSrc;
use std::str::FromStr;
use std::time::{Duration, Instant};

const SMALL: &str = "video/x-raw,format=I420,width=320,height=180,framerate=30/1";
/// 320x180 I420.
const FRAME: usize = 86_400;
const SECOND: u64 = 1_000_000_000;

struct Rig {
    pipeline: gst::Pipeline,
    src: AppSrc,
    queue: gst::Element,
    release: std::sync::mpsc::Sender<()>,
}

/// appsrc ! videorate ! 30 fps ! queue ! a sink that holds its first frame.
/// With `fixed`, the rate's output goes through [`present_only`] as the
/// normaliser's does.
fn rig(fixed: bool) -> Rig {
    let _ = gst::init();
    let pipeline = gst::Pipeline::new();
    let caps = gst::Caps::from_str(SMALL).unwrap();
    let src = AppSrc::builder().caps(&caps).format(gst::Format::Time).build();
    let rate = make("videorate", "rate").unwrap();
    rate.set_property("drop-only", false);
    let canvas = capsfilter("canvas", &caps).unwrap();
    if fixed {
        present_only(&rate.static_pad("src").unwrap());
    }
    let queue = queue_thread("slot-q-test").unwrap();
    let sink = make("fakesink", "behind").unwrap();
    sink.set_property("async", false);
    sink.set_property("sync", false);
    let (release, wait) = std::sync::mpsc::channel::<()>();
    let wait = std::sync::Mutex::new(wait);
    sink.static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        let _ = wait.lock().unwrap().recv_timeout(Duration::from_secs(60));
        gst::PadProbeReturn::Remove
    });
    let all = [src.upcast_ref(), &rate, &canvas, &queue, &sink];
    pipeline.add_many(all).unwrap();
    gst::Element::link_many(all).unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    Rig { pipeline, src, queue, release }
}

/// A frame stamped as `do-timestamp` stamps one: both times, the same.
fn stamped(at: u64) -> gst::Buffer {
    let mut buffer = gst::Buffer::with_size(FRAME).unwrap();
    let b = buffer.get_mut().unwrap();
    b.set_pts(gst::ClockTime::from_nseconds(at));
    b.set_dts(gst::ClockTime::from_nseconds(at));
    buffer
}

/// A frame, then nothing for three seconds (a renderer starved of CPU, or a
/// source held up behind a full queue), then frames again. Answers the
/// queue's level once it has stopped moving: buffers, bytes, time, and
/// whether it is full by its backstop.
fn gap_then_wait(rig: &Rig) -> (u32, u32, u64, bool) {
    for at in [0, 3 * SECOND, 6 * SECOND] {
        rig.src.push_buffer(stamped(at)).unwrap();
    }
    let q = &rig.queue;
    let (mut last, mut since) = (u32::MAX, Instant::now());
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        let now = q.property::<u32>("current-level-buffers");
        if now != last {
            (last, since) = (now, Instant::now());
        } else if since.elapsed() > Duration::from_millis(1500) {
            break;
        }
    }
    let level = (
        q.property::<u32>("current-level-buffers"),
        q.property::<u32>("current-level-bytes"),
        q.property::<u64>("current-level-time"),
        by_backstop(q) && held_by_backstop(q),
    );
    level
}

fn finish(rig: Rig) {
    drop(rig.release);
    let _ = rig.pipeline.set_state(gst::State::Null);
}

/// The failure: every frame `videorate` repeats carries the decode time of
/// the first, so the queue reads zero however many it holds and fills to its
/// byte backstop, 60 frames where a second is 30. In 1080p that is the
/// 186,624,000 bytes the 0.3.25 log showed at `level_ms: 0`.
#[test]
fn repeated_frames_with_one_decode_time_fill_a_queue_to_its_backstop_at_zero() {
    let rig = rig(false);
    let (buffers, bytes, time, backstop) = gap_then_wait(&rig);
    finish(rig);
    let cap = bytes_for(Some(&gst::Caps::from_str(SMALL).unwrap()), 1.0);
    assert_eq!(time, 0, "the level counted time");
    assert!(backstop, "the queue is not full by its backstop");
    assert!(bytes as usize + FRAME > cap as usize, "{bytes} bytes held against {cap}");
    assert!(buffers >= 59, "{buffers} frames held");
}

/// The fix: the same gap with the decode time cleared at the rate's output.
/// The queue reads the presentation times, fills to its second and stops.
#[test]
fn with_decode_times_cleared_the_same_gap_fills_the_queue_by_time() {
    let rig = rig(true);
    let (buffers, bytes, time, backstop) = gap_then_wait(&rig);
    finish(rig);
    assert!(time >= 900_000_000, "only {time} ns held");
    assert!(!backstop, "a cleared stream reached the backstop");
    assert!(buffers <= 33 && (bytes as usize) <= 33 * FRAME, "{buffers} frames, {bytes} bytes");
}

/// Presentation times and everything else about a buffer are left alone.
#[test]
fn only_the_decode_time_is_cleared() {
    let _ = gst::init();
    let pipeline = gst::Pipeline::new();
    let src = AppSrc::builder().caps(&gst::Caps::from_str(SMALL).unwrap()).format(gst::Format::Time).build();
    let sink = gstreamer_app::AppSink::builder().sync(false).build();
    pipeline.add_many([src.upcast_ref(), sink.upcast_ref()]).unwrap();
    src.link(&sink).unwrap();
    present_only(&src.static_pad("src").unwrap());
    pipeline.set_state(gst::State::Playing).unwrap();
    let mut buffer = stamped(5 * SECOND);
    buffer.get_mut().unwrap().set_duration(gst::ClockTime::from_nseconds(SECOND / 30));
    src.push_buffer(buffer).unwrap();
    let sample = sink.try_pull_sample(gst::ClockTime::from_seconds(5)).expect("a frame");
    let _ = pipeline.set_state(gst::State::Null);
    let out = sample.buffer().unwrap();
    assert_eq!(out.dts(), None);
    assert_eq!(out.pts(), Some(gst::ClockTime::from_seconds(5)));
    assert_eq!(out.duration(), Some(gst::ClockTime::from_nseconds(SECOND / 30)));
    assert_eq!(out.size(), FRAME);
}
