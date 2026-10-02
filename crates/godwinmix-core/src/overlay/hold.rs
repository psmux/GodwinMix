//! Keeping a transparent source's carrier off a compositor that the board
//! draws over instead.

use std::sync::Arc;

use gstreamer as gst;
use gstreamer::prelude::*;

use super::layer::Layer;

/// Keep a transparent source's flattened picture off the compositor. Its
/// pad still exists and the scene still writes its place there; the board
/// reads that and draws the real picture, so the compositor must draw nothing.
///
/// On the head of the source's programme branch, so nothing below it in the
/// programme pipeline sees a buffer, and while `layer` is inactive (a clip
/// whose decoder turned out to have no alpha) everything passes as before.
pub fn hold_back(vtee: &gst::Element, layer: Arc<Layer>) {
    if let Some(pad) = vtee.static_pad("sink") {
        hold_back_at(&pad, layer);
    }
}

/// The same, at one pad: the scene preview's compositor pad for the source.
pub fn hold_back_at(pad: &gst::Pad, layer: Arc<Layer>) {
    pad.add_probe(gst::PadProbeType::BUFFER | gst::PadProbeType::BUFFER_LIST, move |_, _| {
        if layer.active() {
            gst::PadProbeReturn::Drop
        } else {
            gst::PadProbeReturn::Ok
        }
    });
}
