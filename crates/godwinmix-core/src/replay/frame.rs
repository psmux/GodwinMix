//! One encoded frame as the ring keeps it: the encoder's buffer, by
//! reference, and its times on the shared clock.

use gstreamer as gst;

/// One encoded frame, picture or sound.
#[derive(Clone)]
pub struct Frame {
    /// When it is shown, in clock nanoseconds.
    pub at: u64,
    /// When it is decoded, where that differs from `at`.
    pub dts: Option<u64>,
    pub duration: u64,
    pub key: bool,
    pub buffer: gst::Buffer,
}

impl Frame {
    pub fn bytes(&self) -> u64 {
        self.buffer.size() as u64
    }

    pub fn end(&self) -> u64 {
        self.at + self.duration
    }
}
