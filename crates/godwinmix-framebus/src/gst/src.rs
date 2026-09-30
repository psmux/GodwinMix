//! `gmxbussrc`: the reader end as a live source. Each buffer's memory is the
//! owner's slot, leased until the buffer is freed; nothing is copied. It
//! waits for an owner that is not there yet, follows one that restarts, and
//! renegotiates when the format changes.

use gstreamer as gst;
use gstreamer_base as gst_base;

glib::wrapper! {
    pub struct BusSrc(ObjectSubclass<imp::BusSrc>)
        @extends gst_base::PushSrc, gst_base::BaseSrc, gst::Element, gst::Object;
}

mod imp;
mod read;
