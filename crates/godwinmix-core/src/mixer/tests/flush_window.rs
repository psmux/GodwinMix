use super::*;

/// A queue, a one pad tee and a valve, as a source's branch meets a slot, with
/// the frames that reach the tee counted. Answers how many came through in
/// the second after a flush of the slot: pushed out of the valve's own pad,
/// or, with `into_queue`, sent into the queue below it as before 2026-10-05.
fn after_a_flush(shut_first: bool, into_queue: bool) -> u64 {
    let _ = gst::init();
    let pipeline = gst::Pipeline::new();
    let make = |f: &str| gst::ElementFactory::make(f).build().unwrap();
    let (src, queue, tee, valve, below, sink) =
        (make("videotestsrc"), make("queue"), make("tee"), make("valve"), make("queue"), make("fakesink"));
    src.set_property("is-live", true);
    sink.set_property("sync", true);
    let all = [&src, &queue, &tee, &valve, &below, &sink];
    pipeline.add_many(all).unwrap();
    gst::Element::link_many(all).unwrap();
    let frames = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let counted = frames.clone();
    tee.static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        counted.fetch_add(1, Ordering::Relaxed);
        gst::PadProbeReturn::Ok
    });
    pipeline.set_state(gst::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    if shut_first {
        valve.set_property("drop", true);
    }
    let out = valve.static_pad("src").unwrap();
    let entry = below.static_pad("sink").unwrap();
    if into_queue {
        gstutil::wake_chain(&entry);
    } else {
        gstutil::wake_below(&out);
    }
    // Long enough for a frame to meet the flushing pad.
    std::thread::sleep(Duration::from_millis(200));
    if into_queue {
        gstutil::resume_chain(&entry);
    } else {
        gstutil::resume_below(&out);
    }
    valve.set_property("drop", false);
    let before = frames.load(Ordering::Relaxed);
    std::thread::sleep(Duration::from_secs(1));
    let after = frames.load(Ordering::Relaxed);
    pipeline.set_state(gst::State::Null).unwrap();
    after - before
}

/// What `Pool::unbind` relies on: with the valve shut before the flush, the
/// source's own queue goes on feeding its tee. The other half shows why it
/// matters, and is reported rather than asserted, because it is GStreamer's
/// behaviour and not this crate's.
#[test]
fn a_valve_shut_before_a_flush_keeps_the_source_branch_running() {
    let shut = after_a_flush(true, false);
    let open = after_a_flush(false, false);
    let before = after_a_flush(false, true);
    eprintln!(
        "frames in the second after the flush: valve shut first {shut}, valve open {open}, \
         valve open and the flush sent into the queue {before}"
    );
    assert!(shut > 10, "the branch stopped although the valve was shut: {shut} frames");
}
