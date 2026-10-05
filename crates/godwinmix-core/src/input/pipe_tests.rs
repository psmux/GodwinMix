//! The Windows pipe source holds a bounded amount and then makes the writer
//! wait. See `pipe_source` for the 12 GB it held on 2026-10-05 when it did
//! not.

use super::*;

/// A downstream that has stopped reading, and a writer pushing raw 1080p
/// frames as fast as it can. Before the fix the writer never waited and the
/// element's queue grew by a frame a push; now the writer parks once the
/// queue is full and the queue stays under its bound.
#[test]
fn a_pipe_source_with_a_stuck_downstream_makes_the_writer_wait() {
    gst::init().unwrap();
    let pipeline = gst::Pipeline::new();
    let src = pipe_source("stuck-src-container").unwrap();
    let sink = make("fakesink", "stuck-sink").unwrap();
    pipeline.add_many([&src, &sink]).unwrap();
    src.link(&sink).unwrap();
    // The sink's pad blocked: what a decode that cannot keep up looks like
    // from here, with nothing ever let through.
    let pad = sink.static_pad("sink").unwrap();
    pad.add_probe(gst::PadProbeType::BLOCK_DOWNSTREAM, |_, _| gst::PadProbeReturn::Ok);
    pipeline.set_state(gst::State::Playing).unwrap();

    let appsrc = src.clone().downcast::<gstreamer_app::AppSrc>().unwrap();
    let pushed = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let counted = pushed.clone();
    let writer = std::thread::spawn(move || {
        let frame = vec![0u8; 3_110_400];
        for _ in 0..200 {
            if appsrc.push_buffer(gst::Buffer::from_slice(frame.clone())).is_err() {
                return;
            }
            counted.fetch_add(1, Ordering::SeqCst);
        }
    });
    std::thread::sleep(Duration::from_secs(1));
    let level: u64 = src.property("current-level-bytes");
    let waiting = !writer.is_finished();
    let frames = pushed.load(Ordering::SeqCst);
    // Taking the pipeline down is what lets a parked writer go, and it must.
    pipeline.set_state(gst::State::Null).unwrap();
    writer.join().unwrap();

    assert!(waiting, "the writer pushed all 200 frames into a source nobody reads");
    assert!(
        level <= PIPE_QUEUE_BYTES + 3_110_400,
        "the source held {level} bytes, over its bound of {PIPE_QUEUE_BYTES}"
    );
    assert!(frames < 20, "{frames} frames went in before the writer was made to wait");
}
