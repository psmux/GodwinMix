//! The frame bus, on Linux and macOS: the camera's decoded frames read in
//! place from shared memory and scaled down to the tracking width. Nothing
//! is encoded or decoded for tracking at all.
//!
//! Formats: NV12 and I420 with their planes back to back (what a decoder
//! writes), BGRA and BGRx. RGBA is read as BGRA; the colours swap, which
//! the tracker's colour histogram does not mind. Anything else, and planes
//! with padding between them, are read as their brightness plane only.

use std::time::Duration;

use godwinmix_framebus::{BusName, Format, Frame, Registry, Subscriber};
use godwinmix_track::{FrameRef, PixelFormat};

use super::{Frames, Picture};

pub struct BusFrames {
    name: String,
    sub: Subscriber,
    width: u32,
}

impl BusFrames {
    pub fn connect(name: &str, width: u32) -> Result<BusFrames, String> {
        let parsed: BusName = name.parse().map_err(|e| format!("'{name}' is not a frame bus name: {e}"))?;
        let registry = Registry::from_env().map_err(|e| e.to_string())?;
        let sub = Subscriber::connect(&registry, &parsed).map_err(|e| format!("cannot read '{name}' on the frame bus: {e}"))?;
        Ok(BusFrames { name: name.to_string(), sub, width })
    }
}

/// The frame as the tracker sees it, without a copy.
fn view<'a>(frame: &'a Frame) -> Option<FrameRef<'a>> {
    let l = frame.layout();
    let contiguous = l.n_planes < 2 || l.offsets[1] == l.strides[0] as u64 * l.height as u64;
    let (format, data) = match l.format {
        Format::Nv12 if contiguous => (PixelFormat::Nv12, frame.data()),
        Format::I420 if contiguous => (PixelFormat::I420, frame.data()),
        Format::Bgra | Format::Bgrx | Format::Rgba => (PixelFormat::Bgra8, frame.data()),
        _ => (PixelFormat::Gray8, frame.plane(0)),
    };
    FrameRef::new(l.width, l.height, l.strides[0] as usize, format, data).ok()
}

impl Frames for BusFrames {
    fn next(&mut self, timeout: Duration) -> Result<Option<Picture>, String> {
        let frame = self.sub.next(timeout).map_err(|e| e.to_string())?;
        Ok(frame.as_ref().and_then(view).map(|f| Picture::scaled(&f, self.width)))
    }

    fn describe(&self) -> String {
        format!("the frame bus '{}'", self.name)
    }
}
