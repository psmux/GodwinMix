//! Building and timing one test pipeline.

use crate::load::sys::process_cpu_ns;
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// What a run cost between its two marks.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    pub buffers: u64,
    pub cpu_ns: u64,
    pub wall: Duration,
    /// Media time covered, from the buffers' timestamps.
    pub media_ns: u64,
}

impl Timing {
    /// Thousandths of a core, and wall milliseconds, per second of media.
    pub fn per_second(&self) -> Option<(f64, f64)> {
        if self.buffers < 2 || self.media_ns == 0 {
            return None;
        }
        let m = self.media_ns as f64;
        Some((self.cpu_ns as f64 / m * 1000.0, self.wall.as_nanos() as f64 / m * 1000.0))
    }
}

/// CPU and wall clock at one moment.
type Mark = (u64, Instant);

#[derive(Default)]
struct Stamp {
    /// The first buffer into the element being timed.
    start: Option<Mark>,
    /// The last buffer out at the sink.
    end: Option<Mark>,
    /// Media time in, from the first buffer's start to the last one's end.
    first_pts: Option<u64>,
    in_end: u64,
    buffers: u64,
}

fn mark() -> Mark {
    (process_cpu_ns().unwrap_or(0), Instant::now())
}

pub fn make(factory: &str) -> Result<gst::Element, String> {
    gst::ElementFactory::make(factory).build().map_err(|_| format!("{factory} is not installed"))
}

/// A moving test picture: the SMPTE bars scrolling sideways, so an encoder
/// has real motion to find and cannot skip every block as unchanged.
pub fn video_source(width: u32, height: u32, fps: u32, frames: u32) -> Result<Vec<gst::Element>, String> {
    let src = make("videotestsrc")?;
    src.set_property("num-buffers", frames as i32);
    src.set_property_from_str("pattern", "smpte");
    src.set_property("horizontal-speed", 8i32);
    let caps = gst::Caps::builder("video/x-raw")
        .field("format", "I420")
        .field("width", width as i32)
        .field("height", height as i32)
        .field("framerate", gst::Fraction::new(fps as i32, 1))
        .build();
    let filter = make("capsfilter")?;
    filter.set_property("caps", &caps);
    Ok(vec![src, filter])
}

pub fn sink() -> Result<gst::Element, String> {
    let s = make("fakesink")?;
    s.set_property("sync", false);
    s.set_property("enable-last-sample", false);
    Ok(s)
}

/// Link `elements` in a line, the last being the sink, and play to the end.
/// Timing runs from the first buffer into `elements[start]` to the last
/// buffer into the sink, so the elements' start up is left out and an
/// encoder that holds frames back and lets them all go at the end is timed
/// for all of its work.
pub fn run(elements: &[gst::Element], start: usize, patience: Duration) -> Result<Timing, String> {
    let pipeline = gst::Pipeline::new();
    pipeline.add_many(elements).map_err(|e| e.to_string())?;
    gst::Element::link_many(elements).map_err(|_| "the elements do not agree on a format".to_string())?;
    let stamp = Arc::new(Mutex::new(Stamp::default()));
    let pad_of = |i: usize| elements.get(i).and_then(|e| e.static_pad("sink")).ok_or("an element has no sink pad");
    let st = stamp.clone();
    pad_of(start)?.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
        let now = mark();
        let (pts, dur) = info.buffer().map(|b| (b.pts().map(|t| t.nseconds()), b.duration().map(|t| t.nseconds()))).unwrap_or((None, None));
        let mut s = st.lock();
        s.start.get_or_insert(now);
        if let Some(p) = pts {
            s.first_pts.get_or_insert(p);
            s.in_end = s.in_end.max(p + dur.unwrap_or(0));
        }
        s.buffers += 1;
        gst::PadProbeReturn::Ok
    });
    let st = stamp.clone();
    pad_of(elements.len() - 1)?.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        st.lock().end = Some(mark());
        gst::PadProbeReturn::Ok
    });
    let outcome = play(&pipeline, patience);
    let _ = pipeline.set_state(gst::State::Null);
    outcome?;
    let s = stamp.lock();
    let (Some(a), Some(b)) = (s.start, s.end) else { return Err("nothing reached the end of the pipeline".into()) };
    let media_ns = s.in_end.saturating_sub(s.first_pts.unwrap_or(0));
    Ok(Timing { buffers: s.buffers, cpu_ns: b.0.saturating_sub(a.0), wall: b.1.duration_since(a.1), media_ns })
}

fn play(pipeline: &gst::Pipeline, patience: Duration) -> Result<(), String> {
    pipeline.set_state(gst::State::Playing).map_err(|_| "the pipeline would not start".to_string())?;
    let bus = pipeline.bus().ok_or("the pipeline has no bus")?;
    let wanted = [gst::MessageType::Eos, gst::MessageType::Error];
    let msg = bus
        .timed_pop_filtered(gst::ClockTime::from_nseconds(patience.as_nanos() as u64), &wanted)
        .ok_or("no end of stream in time")?;
    match msg.view() {
        gst::MessageView::Error(e) => Err(e.error().to_string()),
        _ => Ok(()),
    }
}
