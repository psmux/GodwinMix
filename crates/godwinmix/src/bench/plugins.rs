//! Which GStreamer plugins this process has loaded, for the `other` column.
//!
//! A plugin's shared library and everything it links against stay mapped
//! once it is loaded, so the list says where an idle core's resident memory
//! went without anyone having to read `/proc/<pid>/smaps` on the runner. On
//! Ubuntu loading `libav` alone pulls in FFmpeg and what it links (speech
//! synthesis, SVG, libplacebo, x265), which was most of the gap between an
//! idle core and its budget.

use gstreamer as gst;
use gstreamer::prelude::*;

/// The names of every loaded plugin, sorted.
pub fn loaded() -> Vec<String> {
    let mut names: Vec<String> = gst::Registry::get()
        .plugins()
        .into_iter()
        .filter(|p| p.is_loaded())
        .map(|p| p.plugin_name().to_string())
        .collect();
    names.sort();
    names
}

/// `12 plugins loaded: app, audiomixer, ...`, for a row.
pub fn loaded_line() -> String {
    let names = loaded();
    format!("{} plugins loaded: {}", names.len(), names.join(", "))
}
