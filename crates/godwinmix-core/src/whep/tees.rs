//! The two tees a `whep/output` puts its viewers on: the video as it came,
//! and the sound made Opus once for all of them.

use crate::gstutil::make;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;

pub fn tee(name: &str) -> Result<gst::Element> {
    let t = make("tee", name)?;
    t.set_property("allow-not-linked", true);
    Ok(t)
}

/// Whatever the programme's sound is, decoded once and made Opus once.
pub fn opus_branch(pipeline: &gst::Pipeline, audio: &gst::Element, name: &str) -> Result<gst::Element> {
    let decode = make("decodebin", &format!("{name}-adec"))?;
    let convert = make("audioconvert", &format!("{name}-aconv"))?;
    let resample = make("audioresample", &format!("{name}-ares"))?;
    let caps = gst::Caps::builder("audio/x-raw").field("rate", 48_000i32).field("channels", 2i32).build();
    let filter = crate::gstutil::capsfilter(&format!("{name}-acaps"), &caps)?;
    let opus = make("opusenc", &format!("{name}-opus"))?;
    crate::probe::set_int(&opus, "bitrate", 96_000);
    let atee = tee(&format!("{name}-atee"))?;
    pipeline.add_many([&decode, &convert, &resample, &filter, &opus, &atee]).context("adding the WHEP audio branch")?;
    audio.link(&decode).context("linking the programme audio to its decoder")?;
    gst::Element::link_many([&convert, &resample, &filter, &opus, &atee]).context("linking the Opus encoder")?;
    let sink = convert.static_pad("sink").context("audioconvert has no sink pad")?;
    decode.connect_pad_added(move |_, pad| {
        if !sink.is_linked() {
            let _ = pad.link(&sink);
        }
    });
    Ok(atee)
}
