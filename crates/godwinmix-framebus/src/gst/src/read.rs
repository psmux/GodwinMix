//! What `gmxbussrc` does per buffer: find the owner, take a frame, and wrap
//! the slot as a buffer without copying it.

use std::time::Duration;

use glib::subclass::prelude::*;
use gst_base::prelude::*;
use gstreamer as gst;
use gstreamer_base as gst_base;
use gstreamer_video as gst_video;

use super::imp::BusSrc;
use crate::gst::{audio_caps_of, caps_of, CAPTURED_CAPS};
use crate::{BusName, Error, Frame, Registry, Subscriber};

impl BusSrc {
    pub(super) fn registry(&self) -> Result<(Registry, BusName), Error> {
        let name: BusName = self.name.lock().unwrap().parse()?;
        let dir = self.dir.lock().unwrap().clone();
        let reg = if dir.is_empty() {
            Registry::from_env()?
        } else {
            Registry::new(dir)?
        };
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
        let (pts, duration) = (frame.pts(), frame.duration());
        let mut buffer = gst::Buffer::from_slice(frame);
        let b = buffer.get_mut().unwrap();
        b.set_offset(seq);
        if self.owner_time.load(std::sync::atomic::Ordering::SeqCst) {
            b.set_pts(pts.map(gst::ClockTime::from_nseconds));
        }
        if layout.is_audio() || self.owner_time.load(std::sync::atomic::Ordering::SeqCst) {
            b.set_duration(duration.map(gst::ClockTime::from_nseconds));
        }
        stamp_captured(b, captured);
        if layout.is_audio() {
            self.negotiate_for(&layout, audio_caps_of)?;
            return Ok(buffer);
        }
        let n = layout.n_planes as usize;
        let offsets: Vec<usize> = layout.offsets[..n].iter().map(|&o| o as usize).collect();
        let strides: Vec<i32> = layout.strides[..n].iter().map(|&s| s as i32).collect();
        let format = gst_video::VideoFormat::from_string(layout.format.name());
        gst_video::VideoMeta::add_full(
            b,
            gst_video::VideoFrameFlags::empty(),
            format,
            layout.width,
            layout.height,
            &offsets,
            &strides,
        )
        .map_err(|_| gst::FlowError::Error)?;
        self.negotiate_for(&layout, caps_of)?;
        Ok(buffer)
    }

    /// Set caps for `layout` if they are not the ones in force.
    fn negotiate_for(
        &self,
        layout: &crate::Layout,
        caps: fn(&crate::Layout) -> gst::Caps,
    ) -> Result<(), gst::FlowError> {
        if *self.caps_for.lock().unwrap() != Some(*layout) {
            self.obj()
                .set_caps(&caps(layout))
                .map_err(|_| gst::FlowError::NotNegotiated)?;
            *self.caps_for.lock().unwrap() = Some(*layout);
        }
        Ok(())
    }
}

/// When the owner published the frame, for end to end latency.
fn stamp_captured(b: &mut gst::BufferRef, captured: u64) {
    let caps = gst::Caps::new_empty_simple(CAPTURED_CAPS);
    gst::ReferenceTimestampMeta::add(
        b,
        &caps,
        gst::ClockTime::from_nseconds(captured),
        gst::ClockTime::NONE,
    );
}
