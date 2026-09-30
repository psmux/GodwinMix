use std::sync::{LazyLock, Mutex};

use glib::prelude::*;
use glib::subclass::prelude::*;
use gst::subclass::prelude::*;
use gstreamer as gst;
use gstreamer_base as gst_base;
use gstreamer_video as gst_video;

use crate::gst::template_caps;
use crate::{Publisher, PublisherOptions};

#[derive(Default)]
pub struct BusSink {
    pub(super) settings: Mutex<Settings>,
    pub(super) state: Mutex<Option<(Publisher, gst_video::VideoInfo)>>,
}

pub(super) struct Settings {
    pub name: String,
    pub dir: String,
    pub max_readers: u32,
    pub leases: u32,
}

impl Default for Settings {
    fn default() -> Self {
        let d = PublisherOptions::default();
        Settings {
            name: String::new(),
            dir: String::new(),
            max_readers: d.max_readers as u32,
            leases: d.leases_per_reader as u32,
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for BusSink {
    const NAME: &'static str = "GmxBusSink";
    type Type = super::BusSink;
    type ParentType = gst_base::BaseSink;
}

impl ObjectImpl for BusSink {
    fn properties() -> &'static [glib::ParamSpec] {
        static P: LazyLock<Vec<glib::ParamSpec>> = LazyLock::new(|| {
            vec![
                glib::ParamSpecString::builder("bus-name")
                    .nick("Bus name")
                    .blurb("camera:<id> or channel:<app>/<stream>")
                    .build(),
                glib::ParamSpecString::builder("bus-dir")
                    .nick("Bus directory")
                    .blurb("The registry directory; empty for GODWINMIX_BUS_DIR or the default")
                    .build(),
                glib::ParamSpecUInt::builder("max-readers")
                    .minimum(1)
                    .maximum(32)
                    .default_value(8)
                    .build(),
                glib::ParamSpecUInt::builder("leases")
                    .nick("Frames per reader")
                    .minimum(1)
                    .maximum(8)
                    .default_value(3)
                    .build(),
            ]
        });
        P.as_ref()
    }

    fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
        let mut s = self.settings.lock().unwrap();
        match pspec.name() {
            "bus-name" => s.name = value.get::<Option<String>>().unwrap().unwrap_or_default(),
            "bus-dir" => s.dir = value.get::<Option<String>>().unwrap().unwrap_or_default(),
            "max-readers" => s.max_readers = value.get().unwrap(),
            "leases" => s.leases = value.get().unwrap(),
            _ => unreachable!(),
        }
    }

    fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
        let s = self.settings.lock().unwrap();
        match pspec.name() {
            "bus-name" => s.name.to_value(),
            "bus-dir" => s.dir.to_value(),
            "max-readers" => s.max_readers.to_value(),
            "leases" => s.leases.to_value(),
            _ => unreachable!(),
        }
    }
}

impl GstObjectImpl for BusSink {}

impl ElementImpl for BusSink {
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        static M: LazyLock<gst::subclass::ElementMetadata> = LazyLock::new(|| {
            gst::subclass::ElementMetadata::new(
                "GodwinMix frame bus sink",
                "Sink/Video",
                "Publishes decoded frames on the frame bus for other pipelines and processes",
                "GodwinMix",
            )
        });
        Some(&M)
    }

    fn pad_templates() -> &'static [gst::PadTemplate] {
        static T: LazyLock<Vec<gst::PadTemplate>> = LazyLock::new(|| {
            vec![gst::PadTemplate::new(
                "sink",
                gst::PadDirection::Sink,
                gst::PadPresence::Always,
                &template_caps(),
            )
            .unwrap()]
        });
        T.as_ref()
    }
}
