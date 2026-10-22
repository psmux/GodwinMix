//! The GStreamer elements for one node, made and linked among themselves.
//!
//! ```text
//!   decode   appsrc ─ [parser] ─ decoder ─ [download | caps(CUDA memory)] ─ tee
//!   scale    queue(leaky) ─ [videorate] ─ [deinterlace] ─ [crop] ─ videoscale | cudaconvertscale ─ caps ─ tee
//!   encode   queue(leaky) ─ [videoconvert] ─ encoder ─ parser ─ caps ─ appsink
//!   aconvert queue(leaky) ─ audioconvert ─ audioresample ─ caps ─ tee
//!   aencode  queue(leaky) ─ audioconvert ─ audioresample ─ encoder [─ parser] ─ caps ─ appsink
//! ```
//!
//! Every branch off a tee starts with a small leaky queue on its own thread,
//! so an encoder that cannot keep up loses frames of its own and never holds
//! up the decoder or the other encoders. The catalogue's properties arrive
//! already in the element's own units; one this element version does not
//! have, or cannot read, is skipped with a warning and never fatal.
//!
//! A node marked `"memory": "cuda"` keeps its pictures on the GPU: the
//! decoder hands them on in CUDA memory, the scale is `cudaconvertscale`,
//! and the encoder takes them as they are (`build_video.rs`).

use std::sync::atomic::{AtomicU64, Ordering};

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::{AppSink, AppSrc};
use serde_json::Value;

use super::spec::NodeSpec;

/// A node's elements, first to last, and the ends that matter.
pub struct Built {
    pub elements: Vec<gst::Element>,
    /// Where an upstream tee links in. None for a decode, which reads its own appsrc.
    pub head: Option<gst::Element>,
    /// Where downstream nodes link from.
    pub tee: Option<gst::Element>,
    pub appsrc: Option<AppSrc>,
    pub appsink: Option<AppSink>,
}

/// Frames or buffers a branch holds before it drops its oldest.
const BRANCH_BUFFERS: u32 = 3;

static SERIAL: AtomicU64 = AtomicU64::new(0);

pub(super) fn make(factory: &str) -> Result<gst::Element, String> {
    let n = SERIAL.fetch_add(1, Ordering::Relaxed);
    gst::ElementFactory::make(factory)
        .name(format!("xc{n}-{factory}"))
        .build()
        .map_err(|_| format!("this machine has no GStreamer element `{factory}`, which the plan chose; install its plugin set or ask for another rendition"))
}

pub(super) fn caps(text: &str) -> Result<gst::Element, String> {
    let filter = make("capsfilter")?;
    let caps: gst::Caps = text.parse().map_err(|_| format!("the caps `{text}` do not parse"))?;
    filter.set_property("caps", caps);
    Ok(filter)
}

pub(super) fn leaky_queue() -> Result<gst::Element, String> {
    let q = make("queue")?;
    q.set_property_from_str("leaky", "downstream");
    q.set_property("max-size-buffers", BRANCH_BUFFERS);
    q.set_property("max-size-bytes", 0u32);
    q.set_property("max-size-time", 0u64);
    Ok(q)
}

pub(super) fn tee() -> Result<gst::Element, String> {
    let t = make("tee")?;
    t.set_property("allow-not-linked", true);
    Ok(t)
}

pub fn build(node: &NodeSpec) -> Result<Built, String> {
    match node.kind.as_str() {
        "decode" => decode(node),
        "scale" => super::build_video::scale(node),
        "encode" => super::build_video::encode(node),
        "aconvert" => aconvert(node),
        "aencode" => aencode(node),
        other => Err(format!("the plan asked for a `{other}` node, which this listener does not know")),
    }
}

pub(super) fn element(node: &NodeSpec, key: &str) -> Result<gst::Element, String> {
    let factory = node.text(key).ok_or_else(|| format!("node {} names no {key}", node.id))?;
    make(factory)
}

fn decode(node: &NodeSpec) -> Result<Built, String> {
    let src = AppSrc::builder().format(gst::Format::Time).is_live(true).block(true).max_bytes(4 * 1024 * 1024).build();
    let mut elements: Vec<gst::Element> = vec![src.clone().upcast()];
    if let Some(p) = node.text("parser") {
        elements.push(make(p)?);
    }
    let decoder = element(node, "element")?;
    set_all(&decoder, node.raw.get("props"));
    elements.push(decoder);
    match node.text("download") {
        _ if node.cuda() => elements.push(caps("video/x-raw(memory:CUDAMemory)")?),
        Some(d) => elements.push(make(d)?),
        None => {}
    }
    let tee = tee()?;
    elements.push(tee.clone());
    Ok(Built { elements, head: None, tee: Some(tee), appsrc: Some(src), appsink: None })
}

fn aconvert(node: &NodeSpec) -> Result<Built, String> {
    let text = format!("audio/x-raw,rate={},channels={}", node.number("sample_rate"), node.number("channels"));
    let tee = tee()?;
    let elements = vec![leaky_queue()?, make("audioconvert")?, make("audioresample")?, caps(&text)?, tee.clone()];
    Ok(Built { head: Some(elements[0].clone()), elements, tee: Some(tee), appsrc: None, appsink: None })
}

fn aencode(node: &NodeSpec) -> Result<Built, String> {
    let enc = element(node, "element")?;
    set_all(&enc, node.raw.get("props"));
    let mut elements = vec![leaky_queue()?, make("audioconvert")?, make("audioresample")?, enc];
    if let Some(p) = node.text("parser") {
        elements.push(make(p)?);
    }
    elements.push(caps("audio/mpeg,mpegversion=4,stream-format=raw")?);
    let sink = appsink()?;
    elements.push(sink.clone().upcast());
    Ok(Built { head: Some(elements[0].clone()), elements, tee: None, appsrc: None, appsink: Some(sink) })
}

pub(super) fn appsink() -> Result<AppSink, String> {
    let sink = make("appsink")?.downcast::<AppSink>().map_err(|_| "appsink is not an AppSink".to_string())?;
    sink.set_sync(false);
    sink.set_property("async", false);
    sink.set_max_buffers(64);
    sink.set_drop(true);
    Ok(sink)
}

/// Set each property that this element has and can read, skipping the rest.
pub fn set_all(el: &gst::Element, props: Option<&Value>) {
    let Some(props) = props.and_then(Value::as_object) else { return };
    for (name, value) in props {
        let Some(pspec) = el.find_property(name).filter(|p| p.flags().contains(glib::ParamFlags::WRITABLE)) else {
            eprintln!("transcode: {} has no property `{name}`, skipped", el.name());
            continue;
        };
        let text = match value {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        match glib::Value::deserialize(&text, pspec.value_type()) {
            Ok(v) if v.type_() == pspec.value_type() => el.set_property_from_value(name, &v),
            _ => eprintln!("transcode: {} cannot take `{text}` for `{name}`, skipped", el.name()),
        }
    }
}
