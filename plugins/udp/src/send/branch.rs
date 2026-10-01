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

/// How far one stream may run ahead of the other before the demuxer waits.
const AHEAD_SECS: u64 = 10;

/// Size a branch queue by time alone, and generously.
///
/// `mpegtsmux` writes nothing until every pad has a buffer, so the stream that
/// is ahead waits in its queue for the one that is behind. When an output is
/// added in the middle of a GOP the core's video starts at the next keyframe
/// and its audio starts at once, so the Matroska carries up to a GOP of audio,
/// two seconds by default and more on a busy machine, before the first video
/// frame. A queue's default second filled, the demuxer stopped on it, the FIFO
/// was never read again and the output went silent for good while the core
/// still called it live. Ten seconds is several GOPs and a few megabytes; a
/// stream that is behind by more than that has stopped, and the core's
/// overflow watchdog restarts this plugin when the FIFO stops being read.
fn hold_a_gop_and_more(queue: &gst::Element) {
    queue.set_property("max-size-buffers", 0u32);
    queue.set_property("max-size-bytes", 0u32);
    queue.set_property("max-size-time", AHEAD_SECS * 1_000_000_000);
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
    hold_a_gop_and_more(&queue);
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
