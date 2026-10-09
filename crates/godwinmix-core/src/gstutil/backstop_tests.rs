use super::*;
use crate::gstutil::{make, queue_preview, queue_thread, queue_time};
use gstreamer_app::AppSrc;
use std::str::FromStr;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn caps(text: &str) -> gst::Caps {
    gst::Caps::from_str(text).unwrap()
}

#[test]
fn every_queue_the_helpers_build_has_a_byte_and_a_buffer_cap() {
    let _ = gst::init();
    let built = [
        queue_time("timed", 5.0, true).unwrap(),
        queue_time("instant", 0.0, false).unwrap(),
        queue_thread("thread").unwrap(),
        queue_preview("preview").unwrap(),
    ];
    for q in built {
        let bytes = q.property::<u32>("max-size-bytes");
        let buffers = q.property::<u32>("max-size-buffers");
        assert!(bytes as f64 >= MIN_BYTES, "{} has a byte cap of {bytes}", q.name());
        assert!(buffers > 0, "{} has no buffer cap", q.name());
    }
    assert_eq!(buffers_for(1.0), 200);
    assert_eq!(buffers_for(0.0), MIN_BUFFERS);
}

#[test]
fn raw_video_is_costed_by_the_frame_and_encoded_media_at_a_flat_rate() {
    let _ = gst::init();
    // 1280x720 NV12 is 1,382,400 bytes a frame: 41.5 MB a second at 30.
    let hd = caps("video/x-raw,format=NV12,width=1280,height=720,framerate=30/1");
    assert_eq!(bytes_for(Some(&hd), 1.0), (1_382_400.0 * 30.0 * RAW_HEADROOM) as u32);
    let unknown_rate = caps("video/x-raw,format=NV12,width=1280,height=720,framerate=0/1");
    assert_eq!(bytes_for(Some(&unknown_rate), 1.0), (1_382_400.0 * 60.0 * RAW_HEADROOM) as u32);
    // A queue a twentieth of a second long still holds four 4K frames.
    let uhd = caps("video/x-raw,format=BGRA,width=3840,height=2160,framerate=30/1");
    assert_eq!(bytes_for(Some(&uhd), 0.05), 3840 * 2160 * 4 * 4);
    // A second of stereo 16 bit sound is 192 kB, far under the floor.
    let sound = caps("audio/x-raw,format=S16LE,rate=48000,channels=2,layout=interleaved");
    assert_eq!(bytes_for(Some(&sound), 1.0), MIN_BYTES as u32);
    let h264 = caps("video/x-h264,stream-format=avc,alignment=au");
    assert_eq!(bytes_for(Some(&h264), 2.0), (2.0 * ENCODED_BYTES_PER_SEC) as u32);
    assert_eq!(bytes_for(None, 2.0), (2.0 * ENCODED_BYTES_PER_SEC) as u32);
}

/// A source feeding `queue` whose consumer has stopped: the sink holds its
/// first buffer until the returned sender is dropped.
struct Stalled {
    pipeline: gst::Pipeline,
    src: AppSrc,
    queue: gst::Element,
    release: std::sync::mpsc::Sender<()>,
}

const SMALL: &str = "video/x-raw,format=I420,width=320,height=180,framerate=30/1";
/// 320x180 I420.
const FRAME: usize = 86_400;

fn stalled(queue: gst::Element) -> Stalled {
    let _ = gst::init();
    let pipeline = gst::Pipeline::new();
    let src = AppSrc::builder().caps(&caps(SMALL)).format(gst::Format::Time).block(true).max_bytes(FRAME as u64).build();
    let sink = make("fakesink", "stopped").unwrap();
    sink.set_property("async", false);
    sink.set_property("sync", false);
    let (release, wait) = std::sync::mpsc::channel::<()>();
    let wait = std::sync::Mutex::new(wait);
    sink.static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        let _ = wait.lock().unwrap().recv_timeout(Duration::from_secs(60));
        gst::PadProbeReturn::Remove
    });
    pipeline.add_many([src.upcast_ref(), &queue, &sink]).unwrap();
    gst::Element::link_many([src.upcast_ref(), &queue, &sink]).unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    Stalled { pipeline, src, queue, release }
}

/// Push up to `frames` frames from a thread of its own, stamped or not, and
/// answer how many went in once the count has stopped moving.
fn push_until_held(rig: &Stalled, frames: usize, stamped: bool) -> usize {
    let pushed = Arc::new(AtomicUsize::new(0));
    let (src, count) = (rig.src.clone(), pushed.clone());
    std::thread::spawn(move || {
        for n in 0..frames {
            let mut buffer = gst::Buffer::with_size(FRAME).unwrap();
            if stamped {
                let b = buffer.get_mut().unwrap();
                b.set_pts(gst::ClockTime::from_nseconds(n as u64 * 33_333_333));
                b.set_duration(gst::ClockTime::from_nseconds(33_333_333));
            }
            if src.push_buffer(buffer).is_err() {
                return;
            }
            count.fetch_add(1, Ordering::SeqCst);
        }
    });
    let (mut last, mut since) = (usize::MAX, Instant::now());
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        let now = pushed.load(Ordering::SeqCst);
        if now != last {
            (last, since) = (now, Instant::now());
        } else if since.elapsed() > Duration::from_millis(1500) {
            break;
        }
    }
    last
}

fn finish(rig: Stalled) {
    drop(rig.release);
    let _ = rig.pipeline.set_state(gst::State::Null);
}

/// The failure itself: a source whose buffers carry no timestamps, into a
/// queue whose consumer has stopped. A time limit alone never fills, so this
/// queue held all 600 frames, 52 MB, and would have held a stream's worth a
/// second for as long as the stall lasted. The backstop stops it at its byte
/// limit and holds the source there.
#[test]
fn a_source_that_stops_timestamping_is_held_at_the_backstop() {
    let _ = gst::init();
    let rig = stalled(queue_thread("untimed").unwrap());
    let pushed = push_until_held(&rig, 600, false);
    let q = &rig.queue;
    let (bytes, cap) = (q.property::<u32>("current-level-bytes"), q.property::<u32>("max-size-bytes"));
    let time = q.property::<u64>("current-level-time");
    let full_by_backstop = by_backstop(q);
    finish(rig);
    let expected = bytes_for(Some(&caps(SMALL)), 1.0);
    assert_eq!(cap, expected, "the byte cap was not fitted to the caps");
    assert_eq!(time, 0, "untimestamped buffers were counted in time");
    assert!(bytes <= cap, "{bytes} bytes held against a cap of {cap}");
    assert!(full_by_backstop, "the queue is not full by its backstop");
    // The cap is 60 frames; the sink holds one and the source one more.
    assert!(pushed < 600 && pushed <= (cap as usize / FRAME) + 4, "{pushed} frames went in");
}

/// The queue as it was before the backstop, time limit only, under the same
/// stall: it takes everything. This is the 11.8 GB, at test size.
#[test]
fn a_time_only_queue_takes_everything_an_untimed_source_gives_it() {
    let _ = gst::init();
    let bare = make("queue", "time-only").unwrap();
    bare.set_property("max-size-buffers", 0u32);
    bare.set_property("max-size-bytes", 0u32);
    bare.set_property("max-size-time", 1_000_000_000u64);
    let rig = stalled(bare);
    let pushed = push_until_held(&rig, 300, false);
    let bytes = rig.queue.property::<u32>("current-level-bytes");
    finish(rig);
    assert_eq!(pushed, 300, "a time only queue stopped an untimed source");
    assert!(bytes as usize >= 298 * FRAME, "{bytes} bytes held");
}

/// The control: the same stall with sane timestamps fills by time, a second
/// of frames, well inside the backstop. The backstop changes nothing here.
#[test]
fn a_timestamped_source_still_fills_by_time() {
    let _ = gst::init();
    let rig = stalled(queue_thread("timed").unwrap());
    let pushed = push_until_held(&rig, 600, true);
    let q = &rig.queue;
    let bytes = q.property::<u32>("current-level-bytes");
    let time = q.property::<u64>("current-level-time");
    let full_by_backstop = by_backstop(q);
    finish(rig);
    assert!(time >= 900_000_000, "only {time} ns held");
    assert!(!full_by_backstop, "a sane stream reached the backstop");
    assert!((bytes as usize) < 40 * FRAME && pushed < 40, "{pushed} frames, {bytes} bytes");
}

/// A preview queue keeps its two frames: the backstop does not loosen it.
#[test]
fn a_preview_queue_keeps_its_two_buffers() {
    let _ = gst::init();
    let q = queue_preview("two").unwrap();
    assert_eq!(q.property::<u32>("max-size-buffers"), 2);
}
