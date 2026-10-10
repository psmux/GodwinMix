//! The picture in front of `mpegtsmux`, for the SRT and RIST outputs.
//!
//! Every output reads the same programme encoder through a tee, and the
//! encoder's parser answers in whichever form its first consumer settled on.
//! An RTMP output puts its own parser in front of `flvmux`, which asks for
//! `avc`; an SRT output added after it linked straight into `mpegtsmux`,
//! which takes only byte-stream, and was refused with "Pads do not have
//! common format". So the TS outputs get a parser of their own too, held to
//! byte-stream by a capsfilter, and either order works.

use crate::gstutil::make;
use crate::plugin::output::{link_to_mux, OutputCtx};
use anyhow::{Context, Result};
use godwinmix_protocol::rendition::VideoCodec;
use gstreamer as gst;
use gstreamer::prelude::*;

/// The `mpegtsmux` pad templates, new spelling first.
pub const TS_PADS: &[&str] = &["sink_%d", "sink_%u"];

/// Link `video` into `mux`, through a parser and a byte-stream capsfilter
/// when the picture is H.264 or HEVC, and straight in otherwise.
pub fn link_video(ctx: &OutputCtx<'_>, video: &gst::Element, mux: &gst::Element) -> Result<()> {
    let Some((parser, caps)) = parser_for(codec(ctx, video)) else {
        link_to_mux(video, mux, TS_PADS)?;
        return Ok(());
    };
    let (id, gen) = (ctx.id, ctx.generation);
    let parse = make(parser, &format!("out-{id}-vparse-{gen}"))?;
    let form = make("capsfilter", &format!("out-{id}-vform-{gen}"))?;
    form.set_property("caps", caps.parse::<gst::Caps>().context("the byte-stream caps")?);
    ctx.pipeline.add_many([&parse, &form]).context("adding the picture's parser")?;
    gst::Element::link_many([video, &parse, &form]).context("linking the picture to its parser")?;
    link_to_mux(&form, mux, TS_PADS)?;
    Ok(())
}

/// The parser and the caps `mpegtsmux` takes from it, for the codecs that
/// come in more than one form.
pub fn parser_for(codec: Option<VideoCodec>) -> Option<(&'static str, &'static str)> {
    match codec {
        Some(VideoCodec::H264) => Some(("h264parse", "video/x-h264,stream-format=byte-stream,alignment=au")),
        Some(VideoCodec::H265) => Some(("h265parse", "video/x-h265,stream-format=byte-stream,alignment=au")),
        _ => None,
    }
}

/// The codec of this output's picture: its rendition's when it asked for
/// one, otherwise what the feed offers, and the programme's own H.264 when
/// the feed cannot say yet (a `proxysrc` answers a caps query with anything).
fn codec(ctx: &OutputCtx<'_>, video: &gst::Element) -> Option<VideoCodec> {
    if let Some(v) = ctx.taps.first().and_then(|t| t.video) {
        return Some(v.codec);
    }
    let caps = video.static_pad("src").map(|p| p.query_caps(None));
    match caps.as_ref().filter(|c| !c.is_any()).and_then(|c| c.structure(0)).map(|s| s.name().as_str()) {
        Some("video/x-h265") => Some(VideoCodec::H265),
        Some("video/x-h264") | None => Some(VideoCodec::H264),
        Some(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h264_and_hevc_get_a_byte_stream_parser_and_others_go_straight_in() {
        assert_eq!(parser_for(Some(VideoCodec::H264)).map(|p| p.0), Some("h264parse"));
        assert!(parser_for(Some(VideoCodec::H265)).unwrap().1.contains("byte-stream"));
        assert!(parser_for(Some(VideoCodec::Av1)).is_none());
        assert!(parser_for(None).is_none());
    }
}
