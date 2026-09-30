//! What `gmxbussrc` does as a live source: start, stop, unblock, and hand
//! out the next frame.

use std::sync::atomic::Ordering::SeqCst;

use gst_base::subclass::base_src::CreateSuccess;
use gst_base::subclass::prelude::*;
use gstreamer as gst;
use gstreamer_base as gst_base;

use super::imp::BusSrc;

impl BaseSrcImpl for BusSrc {
    fn start(&self) -> Result<(), gst::ErrorMessage> {
        self.registry()
            .map_err(|e| gst::error_msg!(gst::ResourceError::Settings, ["{e}"]))?;
        Ok(())
    }

    fn stop(&self) -> Result<(), gst::ErrorMessage> {
        self.sub.lock().unwrap().take();
        self.caps_for.lock().unwrap().take();
        Ok(())
    }

    /// Caps are set from the first frame, so there is nothing to agree on
    /// before one arrives.
    fn negotiate(&self) -> Result<(), gst::LoggableError> {
        Ok(())
    }

    fn unlock(&self) -> Result<(), gst::ErrorMessage> {
        self.flushing.store(true, SeqCst);
        Ok(())
    }

    fn unlock_stop(&self) -> Result<(), gst::ErrorMessage> {
        self.flushing.store(false, SeqCst);
        Ok(())
    }
}

impl PushSrcImpl for BusSrc {
    fn create(&self, _buf: Option<&mut gst::BufferRef>) -> Result<CreateSuccess, gst::FlowError> {
        while !self.flushing.load(SeqCst) {
            match self.frame() {
                Ok(Some(f)) => return Ok(CreateSuccess::NewBuffer(self.wrap(f)?)),
                Ok(None) => continue,
                Err(e) => {
                    gst::element_imp_error!(self, gst::ResourceError::Read, ["{e}"]);
                    return Err(gst::FlowError::Error);
                }
            }
        }
        Err(gst::FlowError::Flushing)
    }
}
