//! Probes the transition tests read the programme through, each on the
//! timeline of the element it watches rather than the wall clock.

use crate::mixer::Mixer;
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

/// How many buffers pass this pad from now on.
pub(super) fn count_buffers(element: &gst::Element, pad: &str) -> Arc<std::sync::atomic::AtomicU64> {
    let seen = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let counter = seen.clone();
    element.static_pad(pad).expect("the pad").add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        gst::PadProbeReturn::Ok
    });
    seen
}

/// Every frame a slot's crop lets through from now on: its running time and
/// the width it left the picture at.
pub(super) fn crop_widths(crop: &gst::Element) -> Arc<Mutex<Vec<(u64, i32)>>> {
    let out: Arc<Mutex<Vec<(u64, i32)>>> = Arc::default();
    let src = crop.static_pad("src").expect("a src pad");
    let sticky = src.sticky_event::<gst::event::Segment>(0).and_then(|e| e.segment().downcast_ref::<gst::ClockTime>().cloned());
    let segment = Mutex::new(sticky);
    let record = out.clone();
    src.add_probe(gst::PadProbeType::BUFFER | gst::PadProbeType::EVENT_DOWNSTREAM, move |pad, info| {
        if let Some(gst::EventView::Segment(sg)) = info.event().map(|e| e.view()) {
            *segment.lock() = sg.segment().downcast_ref::<gst::ClockTime>().cloned();
        }
        let rt = info.buffer().and_then(|b| b.pts()).and_then(|pts| segment.lock().as_ref().and_then(|s| s.to_running_time(pts)));
        let width = pad.current_caps().and_then(|c| c.structure(0).and_then(|s| s.get::<i32>("width").ok()));
        if let (Some(rt), Some(width)) = (rt, width) {
            record.lock().push((rt.nseconds(), width));
        }
        gst::PadProbeReturn::Ok
    });
    out
}

/// Where a pad was on the frame composed half way through a window, read on
/// the compositor's own thread as each frame leaves it, and only once the
/// compositor has made a frame past the window.
///
/// Read once by the wall clock instead, a compositor behind the clock had not
/// reached the window yet and the third was still where it started: "part way
/// out the third is off the left edge, at 0", once on a Windows runner and
/// once here under load.
pub(super) async fn xpos_half_way(mix: &Mixer, pad: &gst::Pad, window: (u64, u64)) -> i32 {
    let seen: Arc<Mutex<Vec<(u64, i32)>>> = Arc::default();
    let src = mix.pool.compositor().static_pad("src").expect("the compositor has a src pad");
    let segment = src.sticky_event::<gst::event::Segment>(0).and_then(|e| e.segment().downcast_ref::<gst::ClockTime>().cloned());
    let (record, pad) = (seen.clone(), pad.clone());
    let probe = src.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
        let rt = info.buffer().and_then(|b| b.pts()).and_then(|pts| segment.as_ref().and_then(|s| s.to_running_time(pts)));
        if let Some(rt) = rt {
            record.lock().push((rt.nseconds(), pad.property::<i32>("xpos")));
        }
        gst::PadProbeReturn::Ok
    });
    let past = window.1 + 2 * mix.canvas.frame_duration().nseconds();
    let until = std::time::Instant::now() + Duration::from_secs(30);
    while !seen.lock().iter().any(|(t, _)| *t >= past) {
        assert!(std::time::Instant::now() < until, "the compositor made no frame past the window in 30 s");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    if let Some(probe) = probe {
        src.remove_probe(probe);
    }
    let mid = window.0 + (window.1 - window.0) / 2;
    let frames = seen.lock();
    frames.iter().min_by_key(|(t, _)| t.abs_diff(mid)).map(|(_, x)| *x).expect("a frame was seen")
}
