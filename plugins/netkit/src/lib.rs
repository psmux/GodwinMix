//! Shared plumbing for the GodwinMix network plugins.
//!
//! `srt`, `whip`, `ingest` and `ndi` all do the same four things and none of
//! them is interesting enough to write four times:
//!
//! | Module | What it is for |
//! |---|---|
//! | [`elements`] | is the GStreamer element here, and if not, which package carries it |
//! | [`pipe`] | build a pipeline, watch its bus on its own thread, never block the streaming thread |
//! | [`stats`] | read a number out of a `stats` structure whose field names move between versions |
//! | [`backoff`] | reconnect timing that does not hammer a destination that is down |
//!
//! This crate is a library. It has no `gmx-plugin.toml` and the plugin loader
//! never sees it.

pub mod backoff;
pub mod elements;
pub mod pipe;
pub mod stats;

/// The `gst::init` every plugin needs, done once however many times it is
/// called.
pub fn init() -> Result<(), String> {
    use std::sync::OnceLock;
    static DONE: OnceLock<Result<(), String>> = OnceLock::new();
    DONE.get_or_init(|| gstreamer::init().map_err(|e| e.to_string()))
        .clone()
}
