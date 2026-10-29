//! What a scale does to the picture before it scales it: its pixel shape
//! when the source's is overridden, its fields woven or deinterlaced.

use gstreamer as gst;
use gstreamer::prelude::*;

use super::build::{caps, make};
use super::spec::NodeSpec;

/// Make an interlaced picture progressive: `on` every picture, `auto` only
/// those the decoder marks as interlaced (it marks each one, so a film
/// coded as frames inside an interlaced sequence is left alone), by the
/// node's `method` (YADIF unless it says otherwise), one picture out for
/// each frame in.
///
/// On the GPU the card does it (see [`on_card`]) when this machine has
/// the CUDA library. Otherwise the picture goes down in I420, which YADIF has a fast path
/// for (measured on 1080i, about half the time it takes on NV12), and the
/// result goes back up for the scale. Measured in a headend, a 480i channel
/// costs about 5% of one core and a 1080i one about 9%; the rest of the
/// work, the conversion, the scale and the encode, stays on the card.
pub(super) fn deinterlace(node: &NodeSpec, always: bool) -> Result<Vec<gst::Element>, String> {
    let d = make("deinterlace")?;
    d.set_property_from_str("mode", if always { "interlaced" } else { "auto" });
    d.set_property_from_str("method", node.text("method").unwrap_or("yadif"));
    d.set_property_from_str("fields", "top");
    if !node.cuda() {
        return Ok(vec![d]);
    }
    if on_card_allowed() && super::cuda_deint::available() {
        return on_card(node, always);
    }
    let convert = super::build::on_card(make("cudaconvert")?, node);
    let down = super::build::on_card(make("cudadownload")?, node);
    let up = super::build::on_card(make("cudaupload")?, node);
    // The caps after the download say system memory: without them the
    // download passes CUDA memory through, since `deinterlace` takes any
    // memory in its caps, and then lets every picture by untouched.
    // Back in NV12 on the card: an encoder may read this stage as it is
    // (a 1080i source made at 1080), and NVENC takes no I420.
    let back = super::build::on_card(make("cudaconvert")?, node);
    Ok(vec![
        convert,
        caps("video/x-raw(memory:CUDAMemory),format=I420")?,
        down,
        caps("video/x-raw,format=I420")?,
        d,
        up,
        caps("video/x-raw(memory:CUDAMemory)")?,
        back,
        caps("video/x-raw(memory:CUDAMemory),format=NV12")?,
    ])
}

/// Deinterlacing on the card: the picture converted to I420 there, the
/// kernel run on it in place of `deinterlace`, and back to NV12 for the
/// scale. `GMX_CUDA_DEINTERLACE=off` in the plugin's environment puts it
/// back on the CPU.
fn on_card(node: &NodeSpec, always: bool) -> Result<Vec<gst::Element>, String> {
    let convert = super::build::on_card(make("cudaconvert")?, node);
    let fields = caps("video/x-raw(memory:CUDAMemory),format=I420")?;
    let back = super::build::on_card(make("cudaconvert")?, node);
    let pad = fields.static_pad("src").ok_or("a capsfilter has no src pad")?;
    super::cuda_deint::install(&pad, always, node.text("method") == Some("linear"));
    Ok(vec![convert, fields, back, caps("video/x-raw(memory:CUDAMemory),format=NV12")?])
}

fn on_card_allowed() -> bool {
    std::env::var("GMX_CUDA_DEINTERLACE").map_or(true, |v| v != "off")
}

/// The display shape a node forces on its source, `[16, 9]` or `[4, 3]`.
pub(super) fn aspect(node: &NodeSpec) -> Option<(i32, i32)> {
    let pair = node.raw.get("aspect")?.as_array()?;
    let num = i32::try_from(pair.first()?.as_u64()?).ok()?;
    let den = i32::try_from(pair.get(1)?.as_u64()?).ok()?;
    (num > 0 && den > 0).then_some((num, den))
}

/// Change the caps going into the branch, on every caps event, before
/// anything in it reads them.
///
/// `weave`: take an interlaced picture as the frame it is, both fields
/// woven, which is what "no deinterlace" means: the caps say progressive.
/// `cudaconvertscale` takes nothing else, and a decoder marks a broadcast's
/// pictures `mixed` whether or not a field ever differs.
///
/// `aspect`: the pixel shape that makes the stored picture that display
/// shape, for a source that flags its own wrong. Every caps event gets it,
/// so a source whose size changes keeps the shape asked for.
pub(super) fn reshape(head: &gst::Element, weave: bool, aspect: Option<(i32, i32)>) {
    let Some(pad) = head.static_pad("sink") else { return };
    pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_, info| {
        let Some(gst::PadProbeData::Event(event)) = &info.data else { return gst::PadProbeReturn::Ok };
        let gst::EventView::Caps(c) = event.view() else { return gst::PadProbeReturn::Ok };
        let caps = c.caps();
        let Some(s) = caps.structure(0) else { return gst::PadProbeReturn::Ok };
        let interlaced = s.get::<&str>("interlace-mode").ok().is_some_and(|m| m != "progressive");
        let size = (s.get::<i32>("width").unwrap_or(0), s.get::<i32>("height").unwrap_or(0));
        let par = aspect.filter(|_| size.0 > 0 && size.1 > 0).map(|(w, h)| gst::Fraction::new(w * size.1, h * size.0));
        if !(weave && interlaced) && par.is_none() {
            return gst::PadProbeReturn::Ok;
        }
        let mut changed = caps.to_owned();
        if let Some(s) = changed.make_mut().structure_mut(0) {
            if weave && interlaced {
                s.set("interlace-mode", "progressive");
                s.remove_field("field-order");
            }
            if let Some(par) = par {
                s.set("pixel-aspect-ratio", par);
            }
        }
        info.data = Some(gst::PadProbeData::Event(gst::event::Caps::new(&changed)));
        gst::PadProbeReturn::Ok
    });
}
