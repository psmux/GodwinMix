//! Each stream `webrtcbin` hands out, to the sink for its kind.

use std::sync::Arc;

use gstreamer as gst;
use gstreamer::prelude::*;

use crate::tagger::{self, Shared, Zero};

/// What follows the depayloader for each codec a WHIP publisher may send.
fn chain(encoding: &str) -> Option<&'static [&'static str]> {
    match encoding {
        "H264" => Some(&["rtph264depay", "h264parse"]),
        "OPUS" => Some(&["rtpopusdepay", "opusdec", "audioconvert", "audioresample", "avenc_aac", "aacparse"]),
        _ => None,
    }
}

pub fn attach(pipeline: &gst::Pipeline, pad: &gst::Pad, to: &Shared, zero: &Arc<Zero>) {
    let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
    let encoding = caps
        .structure(0)
        .and_then(|s| s.get::<String>("encoding-name").ok())
        .unwrap_or_default()
        .to_ascii_uppercase();
    let sink = match encoding.as_str() {
        "H264" => tagger::video_sink(to.clone(), zero.clone()),
        "OPUS" => tagger::audio_sink(to.clone(), zero.clone()),
        _ => match gst::ElementFactory::make("fakesink").build() {
            Ok(sink) => sink,
            Err(_) => return,
        },
    };
    let mut elements: Vec<gst::Element> = vec![gst::ElementFactory::make("queue").build().expect("queue is core")];
    for factory in chain(&encoding).unwrap_or(&[]) {
        match gst::ElementFactory::make(factory).build() {
            Ok(e) => elements.push(e),
            // Without an AAC encoder the picture still comes through; the
            // sound is left out rather than the whole publisher refused.
            Err(_) => return link_to_nothing(pipeline, pad),
        }
    }
    elements.push(sink);
    if pipeline.add_many(&elements).is_err() || gst::Element::link_many(&elements).is_err() {
        return;
    }
    for e in &elements {
        let _ = e.sync_state_with_parent();
    }
    if let Some(into) = elements[0].static_pad("sink") {
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
