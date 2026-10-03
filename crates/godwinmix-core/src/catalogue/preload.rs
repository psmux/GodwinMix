//! Loading the chosen codec plugins while the mixer is built, before
//! anything is on air.
//!
//! A plugin's own start up can bring its process down: on an Intel Arc 140T
//! with GStreamer 1.28.6 the qsv plugin corrupts the heap while it registers
//! its encoders, in about one load in four. At start up that costs the show
//! one restart by its station. The same load the first time a camera needs
//! the hardware decoder, an hour into a service, would cost the programme.
//! So the plugins behind the selection are loaded here, whether or not the
//! first source or output wants them yet.

use super::Selection;
use gstreamer as gst;
use gstreamer::prelude::*;
use tracing::{debug, warn};

pub fn selected(sel: &Selection) {
    for element in [&sel.video_decode, &sel.video_encode, &sel.audio_decode, &sel.audio_encode].map(|c| &c.element) {
        load(element);
    }
}

fn load(element: &str) {
    let Some(name) = element.split_whitespace().next() else { return };
    let Some(factory) = gst::ElementFactory::find(name) else { return };
    match factory.load() {
        Ok(_) => debug!(element = name, "codec plugin loaded at start up"),
        Err(e) => warn!(element = name, error = %e, "the codec plugin would not load; the element is made later if it can be"),
    }
}
