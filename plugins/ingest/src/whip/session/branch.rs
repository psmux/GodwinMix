//! Each stream `webrtcbin` hands out, to the sink for its kind.

use std::sync::Arc;

use gstreamer as gst;
use gstreamer::prelude::*;

use super::vp8;
use crate::tagger::{self, Shared, Zero};

/// What follows the depayloader for each codec a WHIP publisher may send.
/// VP8 is not here: its chain is built and configured by `vp8`.
fn chain(encoding: &str) -> Option<&'static [&'static str]> {
    match encoding {
        "H264" => Some(&["rtph264depay", "h264parse"]),
        "OPUS" => Some(&["rtpopusdepay", "opusdec", "audioconvert", "audioresample", "avenc_aac", "aacparse"]),
        _ => None,
    }
}

/// The elements for one stream, first to last, without the queue or the sink.
/// None when one cannot be made, which leaves that stream unread.
fn elements(encoding: &str) -> Option<Vec<gst::Element>> {
    if encoding == "VP8" {
        return vp8::chain();
    }
    let mut out = Vec::new();
    for factory in chain(encoding).unwrap_or(&[]) {
        let e = gst::ElementFactory::make(factory).build().ok()?;
        // Ask the browser for a keyframe when packets are lost, so a gap
        // heals at the next one rather than whenever it chooses.
        if e.find_property("request-keyframe").is_some() {
            e.set_property("request-keyframe", true);
        }
        out.push(e);
    }
    Some(out)
}

pub fn attach(pipeline: &gst::Pipeline, pad: &gst::Pad, to: &Shared, zero: &Arc<Zero>) {
    let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
    let encoding = caps
        .structure(0)
        .and_then(|s| s.get::<String>("encoding-name").ok())
        .unwrap_or_default()
        .to_ascii_uppercase();
    let sink = match encoding.as_str() {
        // VP8 comes out of its chain as H.264, so it goes to the same sink.
        "H264" | "VP8" => tagger::video_sink(to.clone(), zero.clone()),
        "OPUS" => tagger::audio_sink(to.clone(), zero.clone()),
        _ => match gst::ElementFactory::make("fakesink").build() {
            Ok(sink) => sink,
            Err(_) => return,
        },
    };
    let mut all: Vec<gst::Element> = vec![gst::ElementFactory::make("queue").build().expect("queue is core")];
    match elements(&encoding) {
        Some(chain) => all.extend(chain),
        // Without an AAC encoder the picture still comes through; the sound
        // is left out rather than the whole publisher refused.
        None => return link_to_nothing(pipeline, pad),
    }
    all.push(sink);
    if pipeline.add_many(&all).is_err() || gst::Element::link_many(&all).is_err() {
        return;
    }
    for e in &all {
        let _ = e.sync_state_with_parent();
    }
    if let Some(into) = all[0].static_pad("sink") {
        let _ = pad.link(&into);
    }
}

fn link_to_nothing(pipeline: &gst::Pipeline, pad: &gst::Pad) {
    let Ok(sink) = gst::ElementFactory::make("fakesink").property("async", false).build() else { return };
    if pipeline.add(&sink).is_ok() {
        let _ = sink.sync_state_with_parent();
        if let Some(into) = sink.static_pad("sink") {
            let _ = pad.link(&into);
        }
    }
}
