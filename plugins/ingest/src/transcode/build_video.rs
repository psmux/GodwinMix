//! The scale and the encode of a converted picture, on the CPU or kept on
//! the GPU.
//!
//! On the GPU (`"memory": "cuda"`) the scale is `cudaconvertscale`, which
//! pads with bars itself (`add-borders`), and the encoder takes the CUDA
//! picture with no conversion in front of it. A deinterlace or a crop has no
//! CUDA element here, so the core plans those on the CPU.

use gstreamer as gst;
use gstreamer::prelude::*;

use super::build::{appsink, caps, element, leaky_queue, make, set_all, tee, Built};
use super::spec::NodeSpec;

pub fn scale(node: &NodeSpec) -> Result<Built, String> {
    let (num, den) = node.fps().unwrap_or((30, 1));
    let (width, height) = (node.number("width"), node.number("height"));
    let mut elements = vec![leaky_queue()?];
    if node.text("deinterlace").is_none_or(|d| d == "off") {
        weave(&elements[0]);
    }
    // Only a change of rate needs videorate: at the source's own rate it
    // could only drop or repeat a picture whose time wobbled.
    let rate = node.raw.get("rate").and_then(serde_json::Value::as_bool).unwrap_or(true);
    if rate {
        let r = make("videorate")?;
        r.set_property("skip-to-first", true);
        elements.push(r);
    }
    match node.text("deinterlace") {
        Some("on") => elements.push(make("deinterlace")?),
        Some("auto") => {
            let d = make("deinterlace")?;
            d.set_property_from_str("mode", "auto");
            elements.push(d);
        }
        _ => {}
    }
    let fit = node.text("fit").unwrap_or("pad");
    if fit == "crop" {
        let crop = make("aspectratiocrop")?;
        crop.set_property("aspect-ratio", gst::Fraction::new(width as i32, height.max(1) as i32));
        elements.push(crop);
    }
    let cuda = node.cuda();
    let scaler = make(if cuda { "cudaconvertscale" } else { "videoscale" })?;
    if scaler.has_property("add-borders") {
        scaler.set_property("add-borders", fit == "pad");
    }
    if scaler.has_property("border-color") {
        set_all(&scaler, Some(&serde_json::json!({"border-color": node.number("border")})));
    }
    elements.push(scaler);
    let memory = if cuda { "(memory:CUDAMemory)" } else { "" };
    let framerate = if rate { format!(",framerate={num}/{den}") } else { String::new() };
    elements.push(caps(&format!("video/x-raw{memory},width={width},height={height}{framerate},pixel-aspect-ratio=1/1"))?);
    let tee = tee()?;
    elements.push(tee.clone());
    Ok(Built { head: Some(elements[0].clone()), elements, tee: Some(tee), appsrc: None, appsink: None })
}

pub fn encode(node: &NodeSpec) -> Result<Built, String> {
    let enc = element(node, "element")?;
    set_all(&enc, node.raw.get("props"));
    let mut elements = vec![leaky_queue()?];
    if !node.cuda() {
        elements.push(make("videoconvert")?);
    }
    elements.push(enc);
    if let Some(p) = node.text("parser") {
        elements.push(make(p)?);
    }
    // The framing FLV carries each codec in: AVC and HEVC with their
    // configuration record as codec_data, AV1 as OBUs a temporal unit at a time.
    let framing = match node.text("codec").unwrap_or("h264") {
        "h265" => "video/x-h265,stream-format=hvc1,alignment=au",
        "av1" => "video/x-av1,stream-format=obu-stream,alignment=tu",
        _ => "video/x-h264,stream-format=avc,alignment=au",
    };
    elements.push(caps(framing)?);
    let sink = appsink()?;
    elements.push(sink.clone().upcast());
    Ok(Built { head: Some(elements[0].clone()), elements, tee: None, appsrc: None, appsink: Some(sink) })
}

/// Take an interlaced picture as the frame it is, both fields woven, which
/// is what "no deinterlace" means: the caps going into the branch say
/// progressive. `cudaconvertscale` takes nothing else, and a decoder marks
/// a broadcast's pictures `mixed` whether or not a field ever differs.
fn weave(head: &gst::Element) {
    let Some(pad) = head.static_pad("sink") else { return };
    pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, |_, info| {
        let Some(gst::PadProbeData::Event(event)) = &info.data else { return gst::PadProbeReturn::Ok };
        let gst::EventView::Caps(c) = event.view() else { return gst::PadProbeReturn::Ok };
        let caps = c.caps();
        let interlaced = caps.structure(0).and_then(|s| s.get::<&str>("interlace-mode").ok()).is_some_and(|m| m != "progressive");
        if !interlaced {
            return gst::PadProbeReturn::Ok;
        }
        let mut woven = caps.to_owned();
        if let Some(s) = woven.make_mut().structure_mut(0) {
            s.set("interlace-mode", "progressive");
            s.remove_field("field-order");
        }
        info.data = Some(gst::PadProbeData::Event(gst::event::Caps::new(&woven)));
        gst::PadProbeReturn::Ok
    });
}
