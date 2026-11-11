//! Pictures for the follow loop, small: the tracker wants about 320 pixels
//! across, never the full camera picture.
//!
//! Two ways in, cheapest first:
//!
//! * [`bus`] (Linux and macOS): the frame bus, when the camera's picture is
//!   published on it. The decoded frame is read in place from shared memory
//!   and scaled down here: no encode, no decode, no copy of the full frame.
//! * [`mjpeg`]: the core's `/mjpeg/<source>` stream at the tracking width.
//!   Works everywhere. The core makes the stream only while this loop reads
//!   it, so nothing runs when nobody is tracking.

#[cfg(unix)]
pub mod bus;
pub mod mjpeg;

use std::time::Duration;

use godwinmix_track::{FrameRef, PixelFormat};

use crate::core::Core;
use crate::settings::CameraSpec;

/// One picture, owned, ready for the tracker.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub data: Vec<u8>,
}

impl Picture {
    pub fn frame(&self) -> Option<FrameRef<'_>> {
        FrameRef::packed(self.width, self.height, self.format, &self.data).ok()
    }

    /// Scale `frame` to `width` across, keeping its shape, as packed RGB.
    pub fn scaled(frame: &FrameRef, width: u32) -> Picture {
        let w = width.min(frame.width).max(2);
        let h = ((frame.height as u64 * w as u64) / frame.width.max(1) as u64).max(2) as u32;
        let mut data = vec![0u8; (w * h * 3) as usize];
        frame.downscale_rgb(&mut data, w as usize, h as usize);
        Picture { width: w, height: h, format: PixelFormat::Rgb8, data }
    }
}

pub trait Frames: Send {
    /// The newest picture, or `None` when none came within `timeout`. An
    /// error says why there are no pictures and what to do; the next call
    /// tries again.
    fn next(&mut self, timeout: Duration) -> Result<Option<Picture>, String>;
    /// Where the pictures come from, for `track_state`.
    fn describe(&self) -> String;
}

/// The cheapest way to read this camera's picture.
pub fn open(core: Option<&Core>, spec: &CameraSpec, width: u32) -> Result<Box<dyn Frames>, String> {
    if let Some(name) = &spec.bus {
        #[cfg(unix)]
        return bus::BusFrames::connect(name, width).map(|b| Box::new(b) as Box<dyn Frames>);
        #[cfg(not(unix))]
        return Err(format!("camera '{}' names the frame bus '{name}', and this machine has no frame bus: it is Linux and macOS only. Remove `bus` to read the picture from the mixer instead.", spec.id));
    }
    let source = spec.source.as_deref().ok_or_else(|| {
        format!("camera '{}' has no `source`. Set it to the mixer source that shows this camera, with camera_add, so the plugin can see what the camera sees.", spec.id)
    })?;
    let core = core.ok_or("there is no mixer to read pictures from: the plugin was started without GMX_RPC or --url.")?;
    Ok(Box::new(mjpeg::MjpegFrames::new(core.mjpeg_url(source, width), source)))
}
