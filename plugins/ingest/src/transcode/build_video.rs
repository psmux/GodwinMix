//! The scale and the encode of a converted picture, on the CPU or kept on
//! the GPU.
//!
//! On the GPU (`"memory": "cuda"`) the scale is `cudaconvertscale`, which
//! pads with bars itself (`add-borders`), and the encoder takes the CUDA
//! picture with no conversion in front of it. A crop has no CUDA element
//! here, so the core plans it on the CPU. Nor has a deinterlace: GStreamer
//! 1.26 has no CUDA one and its NVDEC decoders always weave, so the
//! picture goes down for `deinterlace` and comes back up (see
//! [`deinterlace`]), and everything else stays on the card.
//!
//! Both scalers fit by the picture's shape, not its stored size: each
//! reads `pixel-aspect-ratio` off its input, so an anamorphic 16:9 SD
//! picture fills a 16:9 frame and a 4:3 one is laid in with bars.

use gstreamer as gst;
use gstreamer::prelude::*;

use super::build::{appsink, caps, element, leaky_queue, make, set_all, tee, Built};
use super::picture::{aspect, deinterlace, reshape};
use super::spec::NodeSpec;

pub fn scale(node: &NodeSpec) -> Result<Built, String> {
    let (num, den) = node.fps().unwrap_or((30, 1));
    let (width, height) = (node.number("width"), node.number("height"));
    let mut elements = vec![leaky_queue()?];
    let cuda = node.cuda();
    let deinterlacing = matches!(node.text("deinterlace"), Some("on" | "auto"));
    reshape(&elements[0], !deinterlacing, aspect(node));
    if deinterlacing {
        elements.extend(deinterlace(node, node.text("deinterlace") == Some("on"))?);
    }
    // Only a change of rate needs videorate: at the source's own rate it
    // could only drop or repeat a picture whose time wobbled. After the
    // deinterlace, which needs the fields in the order they came.
    let rate = node.raw.get("rate").and_then(serde_json::Value::as_bool).unwrap_or(true);
    if rate {
        let r = make("videorate")?;
        r.set_property("skip-to-first", true);
        elements.push(r);
    }
    let fit = node.text("fit").unwrap_or("pad");
    if fit == "crop" {
        // Raw video in front of the crop: straight after the queue, whose
        // caps before anything flows are ANY, `aspectratiocrop` answers a
        // caps query with nothing, and it would not link to the scaler.
        elements.push(caps("video/x-raw")?);
        let crop = make("aspectratiocrop")?;
        crop.set_property("aspect-ratio", gst::Fraction::new(width as i32, height.max(1) as i32));
        elements.push(crop);
    }
    let scaler = make(if cuda { "cudaconvertscale" } else { "videoscale" })?;
    let scaler = if cuda { super::build::on_card(scaler, node) } else { scaler };
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
        // 4:2:0, which every player decodes: given the choice, x264 took
        // the 4:2:2 a converter offered first and made High 4:2:2, which a
        // browser does not play.
        if node.text("codec").unwrap_or("h264") == "h264" {
            elements.push(caps("video/x-raw,format=I420")?);
        }
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
    // The profile the catalogue asks for, which the encoder reads off the
    // caps after it: left to itself NVENC makes Baseline.
    let framing = match node.text("profile") {
        Some(p) => format!("{framing},profile={p}"),
        None => framing.to_string(),
    };
    elements.push(caps(&framing)?);
    let sink = appsink()?;
    elements.push(sink.clone().upcast());
    Ok(Built { head: Some(elements[0].clone()), elements, tee: None, appsrc: None, appsink: Some(sink) })
}
