//! What `gmxbussrc` does per buffer: find the owner, take a frame, and wrap
//! the slot as a buffer without copying it.

use std::time::Duration;

use glib::subclass::prelude::*;
use gst_base::prelude::*;
use gstreamer as gst;
use gstreamer_base as gst_base;
use gstreamer_video as gst_video;

use super::imp::BusSrc;
use crate::gst::{caps_of, CAPTURED_CAPS};
use crate::{BusName, Error, Frame, Registry, Subscriber};

impl BusSrc {
    pub(super) fn registry(&self) -> Result<(Registry, BusName), Error> {
        let name: BusName = self.name.lock().unwrap().parse()?;
        let dir = self.dir.lock().unwrap().clone();
        let reg = if dir.is_empty() { Registry::from_env()? } else { Registry::new(dir)? };
        Ok((reg, name))
    }

    /// The next frame, connecting first if nobody published when we started.
    pub(super) fn frame(&self) -> Result<Option<Frame>, Error> {
        let mut sub = self.sub.lock().unwrap();
        if sub.is_none() {
            let (reg, name) = self.registry()?;
            match Subscriber::connect(&reg, &name) {
                Ok(s) => *sub = Some(s),
                Err(Error::NotFound { .. }) => {
                    drop(sub);
                    std::thread::sleep(Duration::from_millis(50));
                    return Ok(None);
                }
                Err(e) => return Err(e),
            }
        }
        sub.as_mut().unwrap().next(Duration::from_millis(50))
    }

    pub(super) fn wrap(&self, frame: Frame) -> Result<gst::Buffer, gst::FlowError> {
        let layout = frame.layout();
        let (seq, captured) = (frame.seq(), frame.captured_ns());
        let mut buffer = gst::Buffer::from_slice(frame);
        let b = buffer.get_mut().unwrap();
        b.set_offset(seq);
        let n = layout.n_planes as usize;
        let offsets: Vec<usize> = layout.offsets[..n].iter().map(|&o| o as usize).collect();
        let strides: Vec<i32> = layout.strides[..n].iter().map(|&s| s as i32).collect();
        let format = gst_video::VideoFormat::from_string(layout.format.name());
        gst_video::VideoMeta::add_full(b, gst_video::VideoFrameFlags::empty(), format, layout.width, layout.height, &offsets, &strides)
            .map_err(|_| gst::FlowError::Error)?;
        let caps = gst::Caps::new_empty_simple(CAPTURED_CAPS);
        gst::ReferenceTimestampMeta::add(b, &caps, gst::ClockTime::from_nseconds(captured), gst::ClockTime::NONE);
        if *self.caps_for.lock().unwrap() != Some(layout) {
            self.obj().set_caps(&caps_of(&layout)).map_err(|_| gst::FlowError::NotNegotiated)?;
            *self.caps_for.lock().unwrap() = Some(layout);
        }
        Ok(buffer)
    }
}

