//! What `gmxbussink` does with caps and buffers.

use gst_base::subclass::prelude::*;
use gstreamer as gst;
use gstreamer_base as gst_base;
use gstreamer_video as gst_video;

use super::imp::BusSink;
use crate::gst::layout_of;
use crate::{BusName, Publisher, PublisherOptions, Registry};

impl BaseSinkImpl for BusSink {
    fn set_caps(&self, caps: &gst::Caps) -> Result<(), gst::LoggableError> {
        let info = gst_video::VideoInfo::from_caps(caps)
            .map_err(|_| gst::loggable_error!(gst::CAT_RUST, "caps without a video format"))?;
        let layout = layout_of(&info).map_err(|e| gst::loggable_error!(gst::CAT_RUST, "{e}"))?;
        let mut state = self.state.lock().unwrap();
        if let Some((p, old)) = state.as_mut() {
            p.set_layout(layout)
                .map_err(|e| gst::loggable_error!(gst::CAT_RUST, "{e}"))?;
            *old = info;
            return Ok(());
        }
        let s = self.settings.lock().unwrap();
        let name: BusName = s
            .name
            .parse()
            .map_err(|e| gst::loggable_error!(gst::CAT_RUST, "{e}"))?;
        let reg = if s.dir.is_empty() {
            Registry::from_env()
        } else {
            Registry::new(&s.dir)
        };
        let opts = PublisherOptions {
            max_readers: s.max_readers as usize,
            leases_per_reader: s.leases as usize,
            checksum: false,
        };
        let p = reg
            .and_then(|reg| Publisher::create(&reg, &name, layout, opts))
            .map_err(|e| gst::loggable_error!(gst::CAT_RUST, "{e}"))?;
        *state = Some((p, info));
        Ok(())
    }

    fn render(&self, buffer: &gst::Buffer) -> Result<gst::FlowSuccess, gst::FlowError> {
        let mut state = self.state.lock().unwrap();
        let Some((p, info)) = state.as_mut() else {
            return Err(gst::FlowError::NotNegotiated);
        };
        p.push_buffer(buffer, info).map_err(|e| {
            gst::element_imp_error!(self, gst::StreamError::Format, ["{e}"]);
            gst::FlowError::Error
        })?;
        Ok(gst::FlowSuccess::Ok)
    }

    fn stop(&self) -> Result<(), gst::ErrorMessage> {
        self.state.lock().unwrap().take();
        Ok(())
    }
}
