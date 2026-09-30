//! One programme stream from the demuxer to the muxer: a queue and the parser
//! that makes it what MPEG-TS carries (byte stream H.264 with its parameter
//! sets on every keyframe, ADTS AAC, and so on).

use godwinmix_sdk::plugin::Reporter;
use gstreamer as gst;
use gstreamer::prelude::*;

use crate::recv::make;

/// The parser for a stream the programme can carry, or `None` for one MPEG-TS
/// has no mapping for.
fn parser_for(caps: &gst::StructureRef) -> Option<&'static str> {
    let version = caps.get::<i32>("mpegversion").unwrap_or(0);
    Some(match caps.name().as_str() {
        "video/x-h264" => "h264parse",
        "video/x-h265" => "h265parse",
        "video/mpeg" if version == 2 => "mpegvideoparse",
        "audio/mpeg" if version == 4 || version == 2 => "aacparse",
        "audio/mpeg" if version == 1 => "mpegaudioparse",
        "audio/x-ac3" | "audio/x-eac3" => "ac3parse",
        "audio/x-opus" => "opusparse",
        _ => return None,
    })
}

/// Build a branch for each stream as the demuxer finds it. The closure only
/// makes and links elements; it never waits.
pub fn on_streams(demux: &gst::Element, pipeline: &gst::Pipeline, mux: &gst::Element, reporter: Option<Reporter>) {
    let (weak, mux) = (pipeline.downgrade(), mux.clone());
    demux.connect_pad_added(move |_, pad| {
        let Some(pipeline) = weak.upgrade() else { return };
        if let Err(e) = link(&pipeline, pad, &mux) {
            if let Some(r) = &reporter {
                r.error(format!("a programme stream could not be sent: {e}"));
            }
        }
    });
}

fn link(pipeline: &gst::Pipeline, pad: &gst::Pad, mux: &gst::Element) -> Result<(), String> {
    let caps = pad.current_caps().ok_or("a stream appeared with no caps")?;
    let s = caps.structure(0).ok_or("a stream with empty caps")?;
    let Some(parser) = parser_for(s) else {
        // Somewhere to put it, so the demuxer does not stop on "not linked".
        let sink = make("fakesink", "")?;
        pipeline.add(&sink).map_err(|e| e.to_string())?;
        sink.sync_state_with_parent().ok();
        pad.link(&sink.static_pad("sink").ok_or("no sink pad")?).map_err(|e| e.to_string())?;
        return Err(format!("MPEG-TS has no mapping for {}; that stream is left out", s.name()));
    };
    let queue = make("queue", "")?;
    let parse = make(parser, "")?;
    if parse.find_property("config-interval").is_some() {
        // Parameter sets before every keyframe: a receiver that tunes in late
        // can start at the next one.
        parse.set_property("config-interval", -1i32);
    }
    pipeline.add_many([&queue, &parse]).map_err(|e| e.to_string())?;
    queue.link(&parse).map_err(|e| e.to_string())?;
    let target = mux.request_pad_simple("sink_%d").ok_or("mpegtsmux gave no pad")?;
    parse
        .static_pad("src")
        .ok_or("no parser pad")?
        .link(&target)
        .map_err(|e| format!("mpegtsmux would not take {}: {e}", s.name()))?;
    for e in [&queue, &parse] {
        e.sync_state_with_parent().ok();
    }
    pad.link(&queue.static_pad("sink").ok_or("no queue pad")?)
        .map_err(|e| format!("could not link the {} stream: {e}", s.name()))?;
    Ok(())
}
