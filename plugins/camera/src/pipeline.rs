//! Building the camera's pipeline: the source element, then the chain that
//! turns whatever it produces into the canvas contract.
//!
//! ```text
//!  <camera> ! caps ! decodebin ! videoconvert ! videoscale ! videorate !
//!            video/x-raw,format=I420,<canvas> ! queue ! <transport>
//! ```
//!
//! `decodebin` is in there because a camera that will only give 1080p as
//! Motion JPEG or H.264 is common, and because a camera that gives raw frames
//! links straight through it at no cost. `videorate` is in there because the
//! canvas has one frame rate and a camera has another, and the compositor
//! should never be the thing that notices.
//!
//! Which camera, and through which element, is `device`'s question.

use gstreamer as gst;
use gstreamer::prelude::*;

use godwinmix_capture_common::{capture, devices, wiring};
use godwinmix_sdk::wire::{Canvas, Transport};

use crate::device::{self, Chosen, Route};
use crate::settings::Settings;

/// The element the source is attached to, and where the chain begins.
const HEAD: &str = "gmx-devcaps";

/// The chain from the device caps to the canvas caps, without its sink.
// Only the tests ask without a device to consult.
#[cfg(test)]
pub fn chain(settings: &Settings, canvas: Canvas) -> String {
    chain_for(settings, canvas, None, None)
}

/// The chain, asking for `auto` when the settings name no size of their own,
/// and for at least `floor` frames a second when they name no rate.
pub fn chain_for(
    settings: &Settings,
    canvas: Canvas,
    auto: Option<(u32, u32)>,
    floor: Option<u32>,
) -> String {
    format!(
        "capsfilter name={HEAD} caps=\"{}\" ! decodebin name=gmx-decode ! \
         videoconvert ! videoscale ! videorate ! {}",
        settings
            .device_caps_with(canvas, auto, floor)
            .replace('"', ""),
        wiring::canvas_video_caps(canvas.width, canvas.height, canvas.fps)
    )
}

/// The whole pipeline, with the camera attached and the addresses bound, and
/// the name of the element the camera was opened with.
pub fn build(
    settings: &Settings,
    canvas: Canvas,
    transport: Transport,
    address: &str,
    route: Route,
) -> Result<(gst::Pipeline, String), String> {
    // Asked once. Every look at the device monitor costs a probe of every
    // provider, which on Windows was 2.4 s for the first one in a process.
    let chosen = device::choose(settings, route)?;
    let auto = auto_size(settings, &chosen, canvas);
    let floor = match (
        settings.framerate,
        chosen.caps.as_ref(),
        settings.size.or(auto),
    ) {
        (None, Some(caps), Some(size)) => rate_floor(caps, size, canvas.fps),
        _ => None,
    };
    let description = wiring::Wiring::video_only(chain_for(settings, canvas, auto, floor))
        .description(transport)?;
    let pipeline = capture::build(&description)?;
    wiring::bind(&pipeline, transport, address)?;
    attach(&pipeline, &chosen.element)?;
    Ok((pipeline, chosen.via))
}

/// The size to ask for when the settings name none: chosen from what the
/// camera says it can do. None for a forced element, which has no device to
/// ask.
fn auto_size(settings: &Settings, chosen: &Chosen, canvas: Canvas) -> Option<(u32, u32)> {
    if settings.size.is_some() {
        return None;
    }
    devices::pick_size(&chosen.sizes, (canvas.width, canvas.height))
}

/// The lowest frame rate worth taking at `size`: the canvas rate, or the best
/// the device has at that size when that is less. None when the device lists
/// no rate for that size.
pub fn rate_floor(caps: &gst::Caps, size: (u32, u32), canvas_fps: u32) -> Option<u32> {
    let best = caps
        .iter()
        .filter(|s| {
            let w = s.get::<i32>("width").ok();
            let h = s.get::<i32>("height").ok();
            w == Some(size.0 as i32) && h == Some(size.1 as i32)
        })
        .filter_map(fastest)
        .max()?;
    Some(best.min(canvas_fps).max(1))
}

/// The highest whole frame rate one structure offers, however it is written.
fn fastest(s: &gst::StructureRef) -> Option<u32> {
    let whole = |f: gst::Fraction| (f.numer().max(0) / f.denom().max(1)) as u32;
    if let Ok(f) = s.get::<gst::Fraction>("framerate") {
        return Some(whole(f));
    }
    if let Ok(range) = s.get::<gst::FractionRange>("framerate") {
        return Some(whole(range.max()));
    }
    let list = s.get::<gst::List>("framerate").ok()?;
    list.iter()
        .filter_map(|v| v.get::<gst::Fraction>().ok())
        .map(whole)
        .max()
}

fn attach(pipeline: &gst::Pipeline, source: &gst::Element) -> Result<(), String> {
    let head = pipeline
        .by_name(HEAD)
        .ok_or_else(|| format!("the pipeline has no '{HEAD}' to attach the camera to"))?;
    pipeline
        .add(source)
        .map_err(|e| format!("could not put the camera in the pipeline: {e}"))?;
    crate::guard::install(source);
    source.link(&head).map_err(|e| {
        format!(
            "the camera and the pipeline would not agree on a format: {e}. Clear the \
             `resolution` and `framerate` settings and let the camera choose."
        )
    })
}

#[cfg(test)]
mod tests;
