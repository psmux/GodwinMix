//! What the capture plugins share.
//!
//! Four plugins in this repository capture something the operating system
//! owns: a camera, a sound card, a screen, and the programme on its way to a
//! file. They differ in one element each and in their settings. Everything
//! else, they got wrong in the same way the first time, so it is written once
//! here.
//!
//! | Module | What it is for |
//! |---|---|
//! | [`elements`] | pick the first GStreamer element that exists on this machine, with the platform order and the fallbacks |
//! | [`wiring`] | turn a chain of elements into a pipeline description that ends at the transport the core negotiated |
//! | [`capture`] | run that pipeline: a bus watch on its own thread, a frame count, and a `health` with a number in it |
//! | [`devices`] | GStreamer's `DeviceMonitor`, read as `discover` candidates |
//! | [`fifo`] | the programme FIFO an output receives media on, opened without deadlocking the core |
//! | [`space`] | free bytes on the filesystem holding a path |
//!
//! Nothing here blocks a streaming thread, and nothing here allocates per
//! frame. The frame counter is one atomic increment in a pad probe; the bus
//! watch is a thread that wakes ten times a second and usually goes back to
//! sleep.

// Every module but `space` and `fifo` is safe Rust. Those two make system
// calls, for the free space on a disk and for a FIFO that has to be opened
// without waiting for a writer, and there is no safe way to ask for either.
// Both are a handful of lines with the reasoning written above them.
#![deny(unsafe_op_in_unsafe_fn)]

pub mod capture;
pub mod devices;
pub mod elements;
pub mod fifo;
pub mod space;
pub mod wiring;

pub use capture::Capture;
pub use wiring::Wiring;

/// Start GStreamer once, whoever asks first.
///
/// `gst::init` is safe to call twice, but a plugin that forgets it entirely
/// gets an error from the first `parse::launch` that names nothing useful.
pub fn init() -> Result<(), String> {
    gstreamer::init().map_err(|e| {
        format!(
            "GStreamer would not start: {e}. Install GStreamer 1.24 or later \
             (`brew install gstreamer` on macOS, `apt install gstreamer1.0-plugins-good \
             gstreamer1.0-plugins-bad` on Debian) and try again."
        )
    })
}
