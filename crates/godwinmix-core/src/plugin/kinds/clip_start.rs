//! Opening a clip that may carry alpha: the ordinary `uridecodebin` path with
//! an alpha branch standing by, used when the container is one that can hold
//! alpha video or when `params.alpha = true` says so.

use super::clip_alpha::AlphaTap;
use super::{assemble, BuildCtx, Ingest, KindParts, Wiring};
use crate::config::Params;
use crate::plugin::MediaEnds;
use anyhow::Result;
use gstreamer as gst;
use gstreamer::glib;
use gstreamer::prelude::*;
use gstreamer_video as gst_video;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tracing::{info, warn};

/// Containers that can carry a video stream with alpha: WebM and Matroska
/// (VP8 and VP9 alpha), QuickTime (ProRes 4444, Animation, PNG), and GIF.
const MAY_HOLD_ALPHA: &[&str] = &["webm", "mkv", "mov", "qt", "gif"];

/// Whether to stand an alpha branch by for this clip.
pub fn wanted(uri: &str, params: &Params) -> bool {
    match params.get("alpha") {
        Some(toml::Value::Boolean(b)) => *b,
        _ => {
            let lower = uri.to_ascii_lowercase();
            let path = lower.split(['?', '#']).next().unwrap_or("");
            path.rsplit_once('.').is_some_and(|(_, ext)| MAY_HOLD_ALPHA.contains(&ext))
        }
    }
}

/// Whether raw video caps carry an alpha channel.
pub fn has_alpha(caps: &gst::CapsRef) -> bool {
    gst_video::VideoInfo::from_caps(&caps.to_owned()).map(|i| i.has_alpha()).unwrap_or(false)
}

/// Open a clip: with an alpha branch standing by when it may hold alpha,
/// as every clip always was when it cannot.
pub fn open(ctx: &BuildCtx, thumb: bool) -> Result<MediaEnds> {
    if wanted(&ctx.cfg.uri, &ctx.cfg.effective_params()) {
        start(ctx, thumb)
    } else {
        super::uridecode(ctx, thumb, false)
    }
}

/// `uridecodebin` and the alpha branch in one source pipeline.
pub fn start(ctx: &BuildCtx, thumb: bool) -> Result<MediaEnds> {
    let decode = crate::gstutil::make("uridecodebin", &format!("{}-src-uri", ctx.id))?;
    decode.set_property("uri", crate::input::to_uri(&ctx.cfg.uri));
    crate::probe::set_bool(&decode, "use-buffering", true);
    prefer_alpha(&decode);
    let tap = Arc::new(AlphaTap::build(&ctx.id, &ctx.canvas)?);
    let mut els = vec![decode.clone()];
    els.extend(tap.elements());
    assemble(ctx, thumb, Ingest::default().with(els).livesync(false), |w: &Wiring| {
        tap.link()?;
        // Connected before `route`, so it sees each pad first. A pad it takes
        // is linked by the time `route` looks, which then leaves it alone.
        let (id, entry, seen, t) = (ctx.id.clone(), w.norm.video_entry(), w.has_video.clone(), tap.clone());
        decode.connect_pad_added(move |_, pad| {
            let Some(caps) = pad.current_caps() else { return };
            if !caps.structure(0).is_some_and(|s| s.name().starts_with("video/")) || !has_alpha(&caps) {
                return;
            }
            match t.join(pad, &entry) {
                Ok(()) => {
                    seen.store(true, Ordering::Relaxed);
                    info!(source = %id, %caps, "the clip has alpha; it is drawn over the programme");
                }
                Err(e) => warn!(source = %id, error = %e, "the clip has alpha and could not be given its branch"),
            }
        });
        w.route(&decode, w.norm.video_entry(), w.norm.audio_entry());
        Ok(KindParts { layer: Some(tap.layer.clone()), ..KindParts::default() })
    })
}

/// Keep a stream that carries alpha away from a decoder that would drop it.
///
/// A VP8 or VP9 stream with alpha arrives as `codec-alpha=true`, and the
/// decoder that keeps it is `vp8alphadecodebin` or `vp9alphadecodebin`. The
/// core raises the machine's hardware decoder above everything (on a Mac,
/// `vtdec_hw`), which takes the stream too and decodes the colour alone, so
/// the clip came out flat. The same decoder takes ProRes 4444 to `NV12`.
/// For those two kinds of stream, only a decoder that keeps alpha is tried:
/// the alpha bins, and `avdec_prores`, which gives `A444_10LE`.
fn prefer_alpha(decode: &gst::Element) {
    decode.connect("autoplug-select", false, |args| {
        let caps = args[2].get::<gst::Caps>().ok()?;
        let factory = args[3].get::<gst::ElementFactory>().ok()?;
        let s = caps.structure(0)?;
        let carries_alpha = s.get::<bool>("codec-alpha").unwrap_or(false)
            || (s.name() == "video/x-prores" && s.get::<&str>("variant").is_ok_and(|v| v.starts_with("4444")));
        let keeps_alpha = matches!(factory.name().as_str(), "vp8alphadecodebin" | "vp9alphadecodebin" | "avdec_prores");
        let decoder = factory.klass().contains("Decoder");
        let skip = carries_alpha && decoder && !keeps_alpha;
        let result = glib::Type::from_name("GstAutoplugSelectResult").and_then(glib::EnumClass::with_type)?;
        result.to_value(if skip { 2 } else { 0 })
    });
}
