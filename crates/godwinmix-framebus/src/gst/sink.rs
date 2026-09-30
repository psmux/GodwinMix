//! `gmxbussink`: the owner end as an element. It publishes on the first caps
//! it is given, changes format when the caps do, and never blocks a render
//! on a reader.

use gstreamer as gst;
use gstreamer_base as gst_base;

glib::wrapper! {
    pub struct BusSink(ObjectSubclass<imp::BusSink>)
        @extends gst_base::BaseSink, gst::Element, gst::Object;
}

mod base;
mod imp;
