//! Which FLV muxer an RTMP output needs: `flvmux` for H.264, as every RTMP
//! server takes, and `eflvmux` for HEVC, which enhanced RTMP carries and
//! YouTube, Twitch's enhanced broadcasting and current OBS and ffmpeg take.

use crate::plugin::output::OutputCtx;
use anyhow::Result;
use godwinmix_protocol::rendition::VideoCodec;

/// The muxer for the codec this output's rendition asked for, or the
/// programme's own H.264 when it asked for none.
pub fn muxer_for(ctx: &OutputCtx<'_>) -> Result<&'static str> {
    let codec = ctx.taps.first().and_then(|t| t.video).map(|v| v.codec).unwrap_or(VideoCodec::H264);
    muxer_for_codec(codec)
}

pub fn muxer_for_codec(codec: VideoCodec) -> Result<&'static str> {
    match codec {
        VideoCodec::H264 => Ok("flvmux"),
        VideoCodec::H265 => {
            anyhow::ensure!(
                crate::probe::exists("eflvmux"),
                "HEVC over RTMP needs GStreamer's eflvmux (GStreamer 1.24 or later, gst-plugins-good). Ask this output's rendition for H.264 instead."
            );
            Ok("eflvmux")
        }
        other => anyhow::bail!(
            "RTMP here carries H.264 and HEVC, not {other:?}. Ask this output's rendition for one of those, or send {other:?} over SRT, RIST or HLS."
        ),
    }
}

/// The parser in front of the muxer, which turns a byte-stream picture into
/// the `avc` or `hvc1` form FLV carries. The programme's encoder answers
/// with the form its first consumer settled on, and on the Windows runner
/// that was byte-stream, which `flvmux` refuses: "Pads do not have common
/// format", and the output was not attached.
pub fn parser_for(ctx: &OutputCtx<'_>) -> &'static str {
    match ctx.taps.first().and_then(|t| t.video).map(|v| v.codec) {
        Some(VideoCodec::H265) => "h265parse",
        _ => "h264parse",
    }
}

/// On an `eflvmux` pad, write enhanced RTMP (a FourCC per packet, no track
/// id), which is what servers that take HEVC read. The pad's default is
/// legacy FLV, whose HEVC codec id is a private extension nobody else
/// reads. `flvmux` pads have no such property and are left alone.
pub fn enhanced(pad: &gstreamer::Pad) {
    use gstreamer::prelude::*;
    if pad.find_property("flv-track-mode").is_some() {
        pad.set_property_from_str("flv-track-mode", "non-multitrack");
    }
}

#[cfg(test)]
#[path = "flv_tests.rs"]
mod tests;
