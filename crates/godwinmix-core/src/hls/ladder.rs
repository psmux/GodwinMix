//! A ladder made here, for the tests: one raw picture scaled and encoded
//! once per rung with x264, with every rung's keyframes on the same frames.
//!
//! The output itself never does this; the rendition planner makes its
//! rungs. The tests drive the packager with it from a `videotestsrc`, with
//! no planner and no mixer.
//!
//! Keyframes are forced rather than left to the encoder's interval: each
//! rung's encoder input is watched, and the first frame at or past every
//! multiple of `keyframe_ms` of running time is asked to be a keyframe. Every
//! rung sees the same frames with the same times, so every rung picks the
//! same ones, and the segments of every rung start on the same picture.

use crate::gstutil::{self, make};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rung {
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub kbps: u32,
}

impl Rung {
    pub fn new(id: &str, width: u32, height: u32, kbps: u32) -> Rung {
        Rung { id: id.into(), width, height, kbps }
    }
}

/// Scale and encode the raw video on `raw` once per rung. Returns each
/// rung's encoded src pad, unlinked, for [`super::attach`].
pub fn encode(
    pipeline: &gst::Pipeline,
    raw: &gst::Pad,
    rungs: &[Rung],
    keyframe_ms: u32,
    tag: &str,
) -> Result<Vec<(Rung, gst::Pad)>> {
    let tee = make("tee", &format!("{tag}-tee"))?;
    tee.set_property("allow-not-linked", true);
    pipeline.add(&tee).context("adding the ladder tee")?;
    let mut out = Vec::new();
    for rung in rungs {
        let pad = branch(pipeline, &tee, rung, keyframe_ms, &format!("{tag}-{}", rung.id))?;
        out.push((rung.clone(), pad));
    }
    tee.sync_state_with_parent().ok();
    raw.link(&tee.static_pad("sink").context("tee has no sink pad")?).context("linking raw video into the ladder")?;
    Ok(out)
}

fn branch(pipeline: &gst::Pipeline, tee: &gst::Element, rung: &Rung, keyframe_ms: u32, tag: &str) -> Result<gst::Pad> {
    let queue = gstutil::queue_thread(&format!("{tag}-q"))?;
    let scale = make("videoscale", &format!("{tag}-scale"))?;
    let convert = make("videoconvert", &format!("{tag}-convert"))?;
    let caps = gst::Caps::builder("video/x-raw")
        .field("width", rung.width as i32)
        .field("height", rung.height as i32)
        .field("format", "I420")
        .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
        .build();
    let filter = gstutil::capsfilter(&format!("{tag}-caps"), &caps)?;
    let enc = make("x264enc", &format!("{tag}-enc"))?;
    enc.set_property("bitrate", rung.kbps);
    enc.set_property_from_str("tune", "zerolatency");
    enc.set_property_from_str("speed-preset", "veryfast");
    // A backstop only: the probe below decides where keyframes go.
    enc.set_property("key-int-max", 1000u32);
    enc.set_property("option-string", "scenecut=0");
    let chain = [&queue, &scale, &convert, &filter, &enc];
    pipeline.add_many(chain).context("adding a ladder rung")?;
    gst::Element::link_many(chain).with_context(|| format!("linking rung {}", rung.id))?;
    force_keyframes(&enc.static_pad("sink").context("encoder has no sink pad")?, keyframe_ms);
    for el in chain.iter().rev() {
        el.sync_state_with_parent().ok();
    }
    let tee_pad = tee.request_pad_simple("src_%u").context("the ladder tee refused a pad")?;
    tee_pad.link(&queue.static_pad("sink").context("queue has no sink pad")?)?;
    enc.static_pad("src").context("encoder has no src pad")
}

/// Ask for a keyframe on the first frame at or past each multiple of
/// `every_ms` of running time. The event goes into the encoder's own sink
/// pad ahead of the frame it is about, which is where an encoder looks.
pub fn force_keyframes(sink: &gst::Pad, every_ms: u32) {
    let every = u64::from(every_ms.max(1)) * 1_000_000;
    let last = AtomicU64::new(u64::MAX);
    sink.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
        let Some(buf) = info.buffer() else { return gst::PadProbeReturn::Ok };
        let running = pad
            .sticky_event::<gst::event::Segment>(0)
            .and_then(|e| e.segment().downcast_ref::<gst::format::Time>().cloned())
            .and_then(|s| buf.pts().and_then(|t| s.to_running_time(t)));
        let Some(running) = running else { return gst::PadProbeReturn::Ok };
        let slot = running.nseconds() / every;
        if last.swap(slot, Ordering::Relaxed) != slot {
            let event = gstreamer_video::DownstreamForceKeyUnitEvent::builder()
                .running_time(running)
                .all_headers(true)
                .build();
            pad.send_event(event);
        }
        gst::PadProbeReturn::Ok
    });
}
