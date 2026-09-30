//! How many encoder sessions a hardware device opens at once.
//!
//! Consumer NVIDIA cards refuse the ninth (or the fourth, on older
//! drivers); VideoToolbox and VA usually do not refuse at all. The only
//! honest way to know is to open small sessions one at a time until one is
//! refused or the cap is reached, then close them all. One probe per device,
//! with its first hardware encoder: the limit is the device's, not the
//! codec's.

use super::pipeline::{make, sink, video_source};
use super::Candidate;
use crate::calibration::{EncoderCal, Sessions};
use godwinmix_protocol::rendition::{Fps, VideoShape};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long a session gets to produce its first frame before it counts as
/// refused. A working one takes tens of milliseconds.
const PATIENCE: Duration = Duration::from_millis(1500);

pub fn probe_all(candidates: &[Candidate], encs: &mut [EncoderCal], cap: u32, notes: &mut Vec<String>) {
    let mut done: Vec<String> = Vec::new();
    for c in candidates.iter().filter(|c| c.slot.hardware) {
        let Some(device) = c.slot.device.clone() else { continue };
        if done.contains(&device) || !encs.iter().any(|e| e.slot.id == c.slot.id) {
            continue;
        }
        done.push(device.clone());
        let found = probe(c, cap);
        if found.opened == 0 {
            notes.push(format!("{device}: not one session opened while probing, so no limit is known"));
            continue;
        }
        for e in encs.iter_mut().filter(|e| e.slot.device.as_deref() == Some(device.as_str())) {
            e.sessions = Some(found);
        }
    }
}

/// Open sessions of `c` until one fails or `cap` are open; close them all.
pub fn probe(c: &Candidate, cap: u32) -> Sessions {
    let mut open: Vec<gst::Pipeline> = Vec::new();
    let mut refused = false;
    while (open.len() as u32) < cap {
        match session(c) {
            Some(p) => open.push(p),
            None => {
                refused = true;
                break;
            }
        }
    }
    let opened = open.len() as u32;
    for p in open {
        let _ = p.set_state(gst::State::Null);
    }
    Sessions { opened, refused }
}

/// One small live session, running until closed, or None when it would not
/// produce a frame.
fn session(c: &Candidate) -> Option<gst::Pipeline> {
    let mut els = video_source(640, 360, 30, u32::MAX >> 1).ok()?;
    // Paced, so a session holds the device the way a live one does without
    // burning it.
    els[0].set_property("is-live", true);
    els.push(make("videoconvert").ok()?);
    let enc = make(&c.element).ok()?;
    let shape = VideoShape { codec: c.slot.codec, width: 640, height: 360, fps: Fps::whole(30), bitrate_kbps: 1000, keyframe_ms: 2000 };
    (c.configure)(&enc, &shape);
    els.push(enc);
    els.push(sink().ok()?);
    let pipeline = gst::Pipeline::new();
    pipeline.add_many(&els).ok()?;
    gst::Element::link_many(&els).ok()?;
    let got = Arc::new(AtomicBool::new(false));
    let flag = got.clone();
    els.last()?.static_pad("sink")?.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        flag.store(true, Ordering::Relaxed);
        gst::PadProbeReturn::Ok
    });
    if pipeline.set_state(gst::State::Playing).is_err() {
        let _ = pipeline.set_state(gst::State::Null);
        return None;
    }
    let bus = pipeline.bus()?;
    let deadline = Instant::now() + PATIENCE;
    while Instant::now() < deadline {
        if got.load(Ordering::Relaxed) {
            return Some(pipeline);
        }
        if bus.timed_pop_filtered(gst::ClockTime::from_mseconds(10), &[gst::MessageType::Error]).is_some() {
            break;
        }
    }
    let _ = pipeline.set_state(gst::State::Null);
    None
}
