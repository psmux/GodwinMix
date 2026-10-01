//! What each node of a plan is in GStreamer. Every body starts with a leaky
//! queue, so a node that falls behind drops its own frames and never holds
//! the tee it hangs off, which on the programme is the compositor's.
//!
//! Encoders come from the codec catalogue by the planner's slot id, with the
//! catalogue's properties and keyframe rule applied by the same code the
//! programme encoder uses. Nothing here names a codec element.

use super::candidates::vars_for;
use crate::catalogue::apply;
use crate::catalogue::model::Role;
use crate::catalogue::Catalogue;
use crate::gstutil::{self, make};
use anyhow::{Context, Result};
use godwinmix_protocol::rendition::{AudioShape, Fps, VideoShape};
use godwinmix_render::NodeKind;
use gstreamer as gst;

/// How much a node's head queue holds before it drops the oldest.
const HEAD_SECS: f64 = 1.0;

/// A node's elements, head first, not yet in any pipeline.
pub struct Body {
    pub elements: Vec<gst::Element>,
    /// The encoder, where the node has one: the keyframe probe and the
    /// governor's preset go on it.
    pub encoder: Option<gst::Element>,
}

/// A name GStreamer takes for a node id: letters, digits and dashes.
pub fn slug(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

fn fraction(fps: Fps) -> gst::Fraction {
    gst::Fraction::new(fps.num as i32, fps.den.max(1) as i32)
}

/// The elements for one node, or None for a node that has none of its own
/// (the programme source in system memory, and every Mux: an output's own
/// feed is the mux).
pub fn body(
    kind: &NodeKind,
    name: &str,
    cat: &Catalogue,
    gfx: &crate::catalogue::select::GraphicsChoice,
    preset: Option<&str>,
) -> Result<Option<Body>> {
    let head = gstutil::queue_time(&format!("{name}-q"), HEAD_SECS, true)?;
    let mut elements = vec![head];
    let mut encoder = None;
    match kind {
        NodeKind::Source { .. } | NodeKind::Mux { .. } | NodeKind::Copy { .. } | NodeKind::Decode { .. } => {
            if !gfx.is_gpu() || !matches!(kind, NodeKind::Source { .. }) {
                return Ok(None);
            }
            // A GPU canvas comes down once, for every rendition to share.
            if let Some(d) = gfx.download.as_deref().filter(|d| crate::probe::exists(d)) {
                elements.push(make(d, &format!("{name}-download"))?);
            }
            elements.push(make("videoconvert", &format!("{name}-conv"))?);
        }
        NodeKind::Scale { width, height, fps, .. } => {
            elements.push(make("videoscale", &format!("{name}-scale"))?);
            let rate = make("videorate", &format!("{name}-rate"))?;
            // Joined to a programme that has been running for an hour, a
            // videorate that fills from the segment start makes an hour of
            // duplicate frames before the first real one.
            crate::probe::set_bool(&rate, "skip-to-first", true);
            elements.push(rate);
            let caps = gst::Caps::builder("video/x-raw")
                .field("width", *width as i32)
                .field("height", *height as i32)
                .field("framerate", fraction(*fps))
                .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
                .build();
            elements.push(gstutil::capsfilter(&format!("{name}-caps"), &caps)?);
        }
        NodeKind::Encode { shape, encoder: slot, .. } => {
            let enc = video_encoder(cat, &slot.id, shape, name, preset)?;
            elements.push(make("videoconvert", &format!("{name}-conv"))?);
            elements.push(enc.clone());
            let entry = cat.video_entry(&slot.id).context("the planner named an encoder the catalogue does not have")?;
            elements.extend(parser(entry.parser.as_deref(), &format!("{name}-parse"))?);
            encoder = Some(enc);
        }
        NodeKind::AudioConvert { channels, sample_rate, .. } => {
            elements.push(make("audioconvert", &format!("{name}-conv"))?);
            elements.push(make("audioresample", &format!("{name}-resample"))?);
            let caps = gst::Caps::builder("audio/x-raw")
                .field("rate", *sample_rate as i32)
                .field("channels", i32::from(*channels))
                .build();
            elements.push(gstutil::capsfilter(&format!("{name}-caps"), &caps)?);
        }
        NodeKind::AudioEncode { shape, .. } => {
            let (enc, parse) = audio_encoder(cat, shape, name)?;
            elements.push(make("audioconvert", &format!("{name}-conv"))?);
            // From its first buffer: a rung built hours into a programme
            // must not fill those hours with silence. See the programme
            // encoder's own `aenc-rate` in `mixer.rs`.
            let rate = make("audiorate", &format!("{name}-rate"))?;
            crate::probe::set_bool(&rate, "skip-to-first", true);
            elements.push(rate);
            elements.push(enc.clone());
            elements.extend(parse);
            encoder = Some(enc);
        }
    }
    Ok(Some(Body { elements, encoder }))
}

/// The catalogue's encoder for this slot, set for this shape.
fn video_encoder(cat: &Catalogue, id: &str, shape: &VideoShape, name: &str, preset: Option<&str>) -> Result<gst::Element> {
    let entry = cat.video_entry(id).with_context(|| format!("the catalogue has no encoder `{id}`"))?;
    let factory = entry.element(Role::Encode).with_context(|| format!("`{id}` names no encoder element"))?;
    let el = make(factory, &format!("{name}-enc"))?;
    // The keyframe probe forces the ladder's keyframes; the encoder's own
    // interval is a ceiling twice as long, so it never adds one between.
    let ceiling = VideoShape { keyframe_ms: shape.keyframe_ms.saturating_mul(2), ..*shape };
    let vars = vars_for(&ceiling, 0);
    apply::apply(&el, &entry.properties, &vars);
    apply::apply_keyframe(&el, entry.keyframe.as_ref(), &vars);
    if let Some(p) = preset {
        crate::probe::set_enum(&el, "speed-preset", p);
    }
    Ok(el)
}

/// The first installed audio encoder for this codec, set for this shape.
fn audio_encoder(cat: &Catalogue, shape: &AudioShape, name: &str) -> Result<(gst::Element, Vec<gst::Element>)> {
    let reg = crate::catalogue::select::GstRegistry;
    let entry = super::candidates::audio_entries(cat, &reg)
        .into_iter()
        .find(|e| super::candidates::audio_codec(&e.codec) == Some(shape.codec))
        .context("no audio encoder for that codec is installed")?;
    let factory = entry.element(Role::Encode).context("the audio entry names no encoder")?;
    let el = make(factory, &format!("{name}-enc"))?;
    let vars = apply::Vars { audio_bitrate_kbps: i64::from(shape.bitrate_kbps), ..apply::Vars::default() };
    apply::apply(&el, &entry.properties, &vars);
    Ok((el, parser(entry.parser.as_deref(), &format!("{name}-parse"))?))
}

/// The catalogue's parser, with SPS and PPS before every keyframe, as the
/// programme chain has it.
fn parser(name: Option<&str>, element: &str) -> Result<Vec<gst::Element>> {
    let Some(name) = name.filter(|n| crate::probe::exists(n)) else { return Ok(Vec::new()) };
    let el = make(name, element)?;
    crate::probe::set_int(&el, "config-interval", -1);
    Ok(vec![el])
}
