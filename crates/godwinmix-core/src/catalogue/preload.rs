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
//!
//! Five crash dumps from that laptop (2026-10-30) all stop in the Intel
//! driver (igd11dxva64, igddxvacommon64) under `gst_plugin_qsv_register`, on
//! this thread, with no other thread in GStreamer. So `h264-qsv` is now
//! ranked under Media Foundation in `codecs.toml`, and the qsv plugin is
//! loaded only on a machine whose config asks for it.
//!
//! Hardware entries only. The risk is a driver registering its encoders, and
//! a software codec has no driver. Loading those too cost an idle core the
//! whole of gst-libav and what a distribution's FFmpeg links against (speech
//! synthesis, SVG, libplacebo, x265): on the 2026-10-11 ci-linux bench that
//! was the largest file backed share of an idle core's memory, and on a
//! Windows laptop about 29 MB of a 79 MB idle core. A software plugin is
//! loaded the first time something makes its element, as GStreamer does by
//! itself.

use super::Selection;
use gstreamer as gst;
use gstreamer::prelude::*;
use tracing::{debug, warn};

pub fn selected(sel: &Selection) {
    for chosen in [&sel.video_decode, &sel.video_encode, &sel.audio_decode, &sel.audio_encode] {
        if wants_preload(&chosen.accel) {
            load(&chosen.element);
        }
    }
}

/// Whether an entry with this `accel` is loaded at start up.
fn wants_preload(accel: &str) -> bool {
    !accel.eq_ignore_ascii_case("software")
}

fn load(element: &str) {
    let Some(name) = element.split_whitespace().next() else { return };
    let Some(factory) = gst::ElementFactory::find(name) else { return };
    match factory.load() {
        Ok(_) => debug!(element = name, "codec plugin loaded at start up"),
        Err(e) => warn!(element = name, error = %e, "the codec plugin would not load; the element is made later if it can be"),
    }
}

#[cfg(test)]
mod tests {
    use super::wants_preload;

    #[test]
    fn only_hardware_entries_are_loaded_at_start_up() {
        assert!(!wants_preload("software"));
        assert!(wants_preload("qsv"));
        assert!(wants_preload("vaapi"));
    }
}
