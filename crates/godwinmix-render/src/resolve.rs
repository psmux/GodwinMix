//! One request against one source: fill in what the request left out, then
//! decide copy or encode for the video. Pure arithmetic, no nodes yet.

use godwinmix_protocol::rendition::{RenditionRequest, StreamInfo, VideoCodec, VideoShape};

use crate::container::{carries_video, container_slug, fps_text, video_codecs, video_name, video_slug};
use crate::error::PlanError;
use crate::sizing::{default_kbps, size};

/// Fraction either way a bitrate may differ and still be copied.
pub const DEFAULT_TOLERANCE: f32 = 0.25;

#[derive(Debug, Clone)]
pub enum VideoDecision {
    None,
    Copy,
    /// `target.keyframe_ms` is what the request asked for, 0 for no view;
    /// the ladder settles the real one.
    Encode { target: VideoShape, why: String },
}

/// The codec the output gets: the one asked for, else the source's own when
/// the container carries it, else the first the container carries that this
/// machine can encode. The flag says whether the request named it.
pub fn codec_for(
    req: &RenditionRequest,
    info: &StreamInfo,
    available: &[VideoCodec],
) -> Result<(VideoCodec, bool), PlanError> {
    let container = req.container;
    if let Some(codec) = req.video.as_ref().and_then(|v| v.codec) {
        if !carries_video(container, codec) {
            return Err(PlanError::ContainerCodec {
                request: req.id.clone(),
                container: container_slug(container).into(),
                codec: video_slug(codec).into(),
                allowed: video_codecs(container).iter().map(|c| video_slug(*c).into()).collect(),
            });
        }
        return Ok((codec, true));
    }
    let own = info.video.map(|v| v.codec).filter(|c| carries_video(container, *c));
    if let (true, Some(codec)) = (info.encoded, own) {
        return Ok((codec, false));
    }
    Ok((fallback(req, available), false))
}

/// The first codec the container carries that this machine can encode.
fn fallback(req: &RenditionRequest, available: &[VideoCodec]) -> VideoCodec {
    let allowed = video_codecs(req.container);
    allowed.iter().copied().find(|c| available.contains(c)).unwrap_or(allowed[0])
}

pub fn resolve_video(
    req: &RenditionRequest,
    source: &str,
    info: &StreamInfo,
    available: &[VideoCodec],
) -> Result<VideoDecision, PlanError> {
    if req.no_video {
        return Ok(VideoDecision::None);
    }
    let Some(src) = info.video else {
        if req.video.is_some() {
            return Err(missing(req, source, "video"));
        }
        return Ok(VideoDecision::None);
    };
    let want = req.video.clone().unwrap_or_default();
    let (codec, explicit) = codec_for(req, info, available)?;
    let (width, height) = size(want.width, want.height, &src);
    let fps = want.fps.unwrap_or(src.fps);
    let keyframe_ms = want.keyframe_ms.unwrap_or(0);
    let tolerance = want.bitrate_tolerance.unwrap_or(DEFAULT_TOLERANCE);
    let mut target = VideoShape { codec, width, height, fps, bitrate_kbps: 0, keyframe_ms };
    let why = mismatch(req, info, &src, &target, want.bitrate_kbps, tolerance);
    let Some(why) = why else {
        return Ok(VideoDecision::Copy);
    };
    if !explicit && !available.contains(&target.codec) {
        target.codec = fallback(req, available);
    }
    target.bitrate_kbps = want.bitrate_kbps.unwrap_or_else(|| default_kbps(&target));
    Ok(VideoDecision::Encode { target, why })
}

pub fn missing(req: &RenditionRequest, source: &str, track: &str) -> PlanError {
    PlanError::MissingTrack { request: req.id.clone(), source: source.into(), track: track.into() }
}

/// The first reason the source cannot be copied, or `None` when it can.
fn mismatch(
    req: &RenditionRequest,
    info: &StreamInfo,
    src: &VideoShape,
    t: &VideoShape,
    kbps: Option<u32>,
    tolerance: f32,
) -> Option<String> {
    if !info.encoded {
        return Some("the source is raw frames, which every output must encode".into());
    }
    if !carries_video(req.container, src.codec) {
        let c = container_slug(req.container);
        return Some(format!("{c} cannot carry the source's {}", video_name(src.codec)));
    }
    if src.codec != t.codec {
        return Some(format!("the source is {} and this output wants {}", video_name(src.codec), video_name(t.codec)));
    }
    if (src.width, src.height) != (t.width, t.height) {
        return Some(format!(
            "the source is {}x{} and this output wants {}x{}",
            src.width, src.height, t.width, t.height
        ));
    }
    if src.fps.as_f64() != t.fps.as_f64() {
        return Some(format!("the source runs at {} fps and this output wants {}", fps_text(src.fps), fps_text(t.fps)));
    }
    if let (Some(want), true) = (kbps, src.bitrate_kbps > 0) {
        let off = (f64::from(src.bitrate_kbps) - f64::from(want)).abs();
        if off > f64::from(want) * f64::from(tolerance) {
            return Some(format!(
                "the source runs at {} kbit/s and this output wants {want}",
                src.bitrate_kbps
            ));
        }
    }
    if t.keyframe_ms > 0 && src.keyframe_ms > 0 && t.keyframe_ms != src.keyframe_ms {
        return Some(format!(
            "the source has a keyframe every {} ms and this output wants one every {} ms",
            src.keyframe_ms, t.keyframe_ms
        ));
    }
    None
}

