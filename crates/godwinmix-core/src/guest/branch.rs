//! What a guest's session does with each stream webrtcbin hands it.
//!
//! ```text
//!   H.264  rtph264depay ──► h264parse ──► decoder ─┐
//!   VP8    rtpvp8depay ──► vp8dec ─────────────────┴► videoconvert ──► videorate ──► I420 at the canvas rate ──► intervideosink
//!   Opus   rtpopusdepay ──► opusdec ──► audioconvert ──► audioresample ──► the canvas's sound ──► interaudiosink
//! ```
//!
//! The decoder is named rather than left to `decodebin`, which on a Mac picks
//! the VideoToolbox one and hands frames back through a GL context this
//! thread does not have. The picture keeps the size it came at; the source's
//! normaliser scales it to the canvas, as it does for any camera. The rate is
//! fixed here because a browser's H.264 says nothing about one, and the
//! inter source paces itself by what its caps say.

use super::Shape;
use crate::gstutil::make;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;

/// The decoders tried for H.264, software first: every one of them works on
/// a thread with no main loop.
const H264_DECODERS: &[&str] = &["avdec_h264", "openh264dec"];

/// The video codec the answer takes: H.264 when the offer has it, else VP8.
pub fn video_codec(offer: &str) -> Option<&'static str> {
    let has = |name: &str| offer.lines().any(|l| l.starts_with("a=rtpmap:") && l.contains(name));
    let h264 = has("H264/") && H264_DECODERS.iter().any(|d| crate::probe::exists(d));
    if h264 {
        Some("H264")
    } else if has("VP8/") && crate::probe::exists("vp8dec") {
        Some("VP8")
    } else {
        None
    }
}

/// The receive caps for the video transceiver.
pub fn video_caps(codec: &str) -> String {
    match codec {
        "H264" => "application/x-rtp,media=video,encoding-name=H264,clock-rate=90000,packetization-mode=(string)1".into(),
        _ => "application/x-rtp,media=video,encoding-name=VP8,clock-rate=90000".into(),
    }
}

/// Build the chain for one of webrtcbin's source pads and link it. Runs on
/// a streaming thread, so it builds and links and nothing else.
pub fn attach(pipeline: &gst::Pipeline, pad: &gst::Pad, channel: &str, shape: Shape) {
    let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
    let encoding = caps.structure(0).and_then(|s| s.get::<String>("encoding-name").ok()).unwrap_or_default();
    let built = match encoding.as_str() {
        "H264" | "VP8" => video(pipeline, &encoding, channel, shape),
        "OPUS" => audio(pipeline, channel, shape),
        other => {
            tracing::warn!(encoding = other, "a guest sent a stream that is neither the picture nor the sound; it is left unread");
            return;
        }
    };
    let linked = built.and_then(|head| {
        let sink = head.static_pad("sink").context("a depayloader with no sink pad")?;
        pad.link(&sink).map(|_| ()).context("linking webrtcbin to the guest's chain")
    });
    if let Err(e) = linked {
        tracing::error!(%channel, %encoding, error = %format!("{e:#}"), "a guest's stream could not be read");
    }
}

fn video(pipeline: &gst::Pipeline, encoding: &str, channel: &str, shape: Shape) -> Result<gst::Element> {
    let mut chain = if encoding == "H264" {
        let depay = make("rtph264depay", &format!("{channel}-vdepay"))?;
        crate::probe::set_bool(&depay, "request-keyframe", true);
        crate::probe::set_bool(&depay, "wait-for-keyframe", true);
        let decoder = H264_DECODERS.iter().find(|d| crate::probe::exists(d)).context("no H.264 decoder")?;
        vec![depay, make("h264parse", &format!("{channel}-vparse"))?, make(decoder, &format!("{channel}-vdec"))?]
    } else {
        vec![make("rtpvp8depay", &format!("{channel}-vdepay"))?, make("vp8dec", &format!("{channel}-vdec"))?]
    };
    let fixed = gst::Caps::builder("video/x-raw").field("format", "I420").field("framerate", gst::Fraction::new(shape.fps, 1)).build();
    chain.push(make("videoconvert", &format!("{channel}-vconv"))?);
    chain.push(make("videorate", &format!("{channel}-vrate"))?);
    chain.push(crate::gstutil::capsfilter(&format!("{channel}-vcaps"), &fixed)?);
    let sink = make("intervideosink", &format!("{channel}-vsink"))?;
    sink.set_property("channel", channel);
    chain.push(sink);
    finish(pipeline, chain)
}

fn audio(pipeline: &gst::Pipeline, channel: &str, shape: Shape) -> Result<gst::Element> {
    let caps = crate::caps::CanvasCaps {
        width: 0,
        height: 0,
        fps: gst::Fraction::new(shape.fps, 1),
        sample_rate: shape.rate,
        channels: shape.channels,
    }
    .audio();
    let sink = make("interaudiosink", &format!("{channel}-asink"))?;
    sink.set_property("channel", channel);
    let chain = vec![
        make("rtpopusdepay", &format!("{channel}-adepay"))?,
        make("opusdec", &format!("{channel}-adec"))?,
        make("audioconvert", &format!("{channel}-aconv"))?,
        make("audioresample", &format!("{channel}-ares"))?,
        crate::gstutil::capsfilter(&format!("{channel}-acaps"), &caps)?,
        sink,
    ];
    finish(pipeline, chain)
}

/// Add `chain`, link it, bring it up from its far end, and answer its head.
fn finish(pipeline: &gst::Pipeline, chain: Vec<gst::Element>) -> Result<gst::Element> {
    pipeline.add_many(&chain).context("adding a guest's chain")?;
    gst::Element::link_many(&chain).context("linking a guest's chain")?;
    for el in chain.iter().rev() {
        el.sync_state_with_parent().with_context(|| format!("starting {}", el.name()))?;
    }
    Ok(chain[0].clone())
}
