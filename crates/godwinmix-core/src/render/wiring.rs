//! Joining a node's body to the graph and taking it out again: the element
//! surgery `graph` does on the mixer thread, and the NULL that waits, off it.

use super::elements::Body;
use super::keyframes;
use crate::gstutil;
use anyhow::{Context, Result};
use godwinmix_render::{Node, NodeKind, Plan, Track};
use gstreamer as gst;
use gstreamer::prelude::*;

/// A body in the pipeline.
pub struct Live {
    pub elements: Vec<gst::Element>,
    /// The pad on the upstream outlet the body's head hangs off.
    pub upstream: Option<(gst::Element, gst::Pad)>,
}

/// Add a body, link it to its outlet, bring it up and only then hang it off
/// the tee it reads.
pub fn link(pipeline: &gst::Pipeline, av_offset_ns: i64, body: Body, upstream: &gst::Element, outlet: &gst::Element, node: &Node, plan: &Plan) -> Result<Live> {
    let els = body.elements;
    pipeline.add_many(&els).context("adding a rendition node")?;
    gst::Element::link_many(&els).context("linking a rendition node")?;
    els.last().context("an empty node")?.link(outlet).context("linking a node to its outlet")?;
    if let (Some(enc), NodeKind::Encode { source, .. }) = (&body.encoder, &node.kind) {
        let ms = plan.keyframe_ms.get(source).copied().unwrap_or(godwinmix_render::DEFAULT_KEYFRAME_MS);
        keyframes::align(enc, ms);
        // On the head queue rather than the encoder, as the programme
        // chain has it on its encoder: an offset on the pad the keyframe
        // request crosses would move the time it asks for.
        if let Some(pad) = els[0].static_pad("src") {
            pad.set_offset(av_offset_ns);
        }
    }
    // Tail first, and up before it is linked to a live tee: a flushing
    // pad handed to a tee pauses its source for good.
    for el in els.iter().rev() {
        el.sync_state_with_parent().ok();
    }
    let head = els[0].static_pad("sink").context("a node head with no sink")?;
    let pad = upstream.request_pad_simple("src_%u").context("the upstream tee refused a pad")?;
    pad.link(&head).context("linking a node to what it reads")?;
    Ok(Live { elements: els, upstream: Some((upstream.clone(), pad)) })
}


pub fn is_audio(kind: &NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::AudioConvert { .. } | NodeKind::AudioEncode { .. } | NodeKind::Decode { track: Track::Audio, .. } | NodeKind::Copy { track: Track::Audio, .. }
    )
}

/// Unlink first, so nothing is pushed into an element on its way to NULL.
pub fn take_down(pipeline: &gst::Pipeline, live: Live) {
    if let Some((_, pad)) = &live.upstream {
        if let Some(peer) = pad.peer() {
            let _ = pad.unlink(&peer);
        }
    }
    retire(pipeline, live.elements, live.upstream);
}

/// Out of the programme here, which waits for nothing, and down to NULL on
/// a thread of its own, because NULL joins each element's streaming thread
/// and a rendition's encoder may be a hardware one whose driver decides how
/// long that takes. The tee pad goes back last, once nothing pushes into it.
pub fn retire(pipeline: &gst::Pipeline, elements: Vec<gst::Element>, upstream: Option<(gst::Element, gst::Pad)>) {
    for el in &elements {
        el.set_locked_state(true);
        let _ = pipeline.remove(el);
    }
    let name = elements.first().map(|e| e.name().to_string()).unwrap_or_default();
    let started = crate::mixer::offload::run("rendition-stop", &name, move || {
        for el in &elements {
            let _ = el.set_state(gst::State::Null);
        }
        if let Some((tee, pad)) = upstream {
            tee.release_request_pad(&pad);
        }
    });
    if !started {
        tracing::warn!(node = %name, "no thread to take a rendition node down on; it is left out of the programme as it is");
    }
}

/// A node's outlet: the tee its consumers hang off. Outlives the body.
pub fn outlet(name: &str) -> Result<gst::Element> {
    let tee = gstutil::make("tee", &format!("{name}-tee"))?;
    tee.set_property("allow-not-linked", true);
    Ok(tee)
}
