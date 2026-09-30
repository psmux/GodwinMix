//! The two things the packager learns from caps: which parser to put in
//! front of `cmafmux`, and what a playlist says about the rung.

use super::track::Track;
use crate::gstutil::make;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::Arc;
use tracing::warn;

/// Keep the track's codec, size and rate in step with the muxer's input.
pub fn watch_caps(mux: &gst::Element, track: Arc<Track>) {
    let Some(pad) = mux.static_pad("sink") else { return };
    pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_, info| {
        if let Some(gst::EventView::Caps(c)) = info.event().map(|e| e.view()) {
            describe(&track, c.caps());
        }
        gst::PadProbeReturn::Ok
    });
}

fn describe(track: &Track, caps: &gst::CapsRef) {
    let Some(s) = caps.structure(0) else { return };
    let codecs = gstreamer_pbutils::functions::codec_utils_caps_get_mime_codec(caps)
        .map(|c| c.to_string())
        .unwrap_or_default();
    track.update_info(|info| {
        info.codecs = codecs;
        info.width = s.get::<i32>("width").unwrap_or(0).max(0) as u32;
        info.height = s.get::<i32>("height").unwrap_or(0).max(0) as u32;
        info.fps = s.get::<gst::Fraction>("framerate").ok().map(|f| (f.numer(), f.denom()));
        info.channels = s.get::<i32>("channels").unwrap_or(0).max(0) as u32;
    });
}

/// The parser that turns what an encoder or a demuxer wrote into what
/// `cmafmux` takes.
fn parser_for(caps: &gst::CapsRef) -> Option<&'static str> {
    match caps.structure(0)?.name().as_str() {
        "video/x-h264" => Some("h264parse"),
        "video/x-h265" => Some("h265parse"),
        "video/x-av1" => Some("av1parse"),
        "audio/mpeg" => Some("aacparse"),
        "audio/x-opus" => Some("opusparse"),
        _ => None,
    }
}

/// Put the right parser between `queue` and `mux` when the first caps
/// reach the queue's output. Done in the probe, before the caps event goes
/// on, so the parser sees the caps like any other element would.
pub fn link_parser_on_caps(queue: &gst::Element, mux: &gst::Element, tag: &str) -> Result<()> {
    let src = queue.static_pad("src").context("queue has no src pad")?;
    let (mux, tag) = (mux.clone(), tag.to_string());
    src.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |pad, info| {
        let Some(gst::EventView::Caps(c)) = info.event().map(|e| e.view()) else {
            return gst::PadProbeReturn::Ok;
        };
        if pad.is_linked() {
            return gst::PadProbeReturn::Remove;
        }
        if let Err(e) = insert_parser(pad, c.caps(), &mux, &tag) {
            warn!(error = %e, packager = %tag, "HLS packager could not take this stream");
        }
        gst::PadProbeReturn::Remove
    });
    Ok(())
}

fn insert_parser(src: &gst::Pad, caps: &gst::CapsRef, mux: &gst::Element, tag: &str) -> Result<()> {
    let factory = parser_for(caps).with_context(|| format!("HLS carries H.264, HEVC, AV1, AAC or Opus, not {caps}"))?;
    let parser = make(factory, &format!("{tag}-parse"))?;
    let bin = mux.parent().and_then(|p| p.downcast::<gst::Bin>().ok()).context("the muxer is in no bin")?;
    bin.add(&parser).context("adding the parser")?;
    parser.link(mux).with_context(|| format!("{factory} will not feed cmafmux"))?;
    parser.sync_state_with_parent().ok();
    let sink = parser.static_pad("sink").context("parser has no sink pad")?;
    src.link(&sink).context("linking the queue into the parser")?;
    Ok(())
}
