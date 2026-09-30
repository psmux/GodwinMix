use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::{LazyLock, Mutex};

use glib::prelude::*;
use glib::subclass::prelude::*;
use gst::subclass::prelude::*;
use gst_base::prelude::*;
use gstreamer as gst;
use gstreamer_base as gst_base;

use crate::gst::template_caps;
use crate::{Layout, Subscriber};

#[derive(Default)]
pub struct BusSrc {
    pub(super) name: Mutex<String>,
    pub(super) dir: Mutex<String>,
    pub(super) sub: Mutex<Option<Subscriber>>,
    pub(super) caps_for: Mutex<Option<Layout>>,
    /// Stamp buffers with the owner's timestamps rather than on arrival.
    pub(super) owner_time: AtomicBool,
    pub(super) flushing: AtomicBool,
}

#[glib::object_subclass]
impl ObjectSubclass for BusSrc {
    const NAME: &'static str = "GmxBusSrc";
    type Type = super::BusSrc;
    type ParentType = gst_base::PushSrc;
}

impl ObjectImpl for BusSrc {
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
                glib::ParamSpecString::builder("timestamps")
                    .nick("Timestamps")
                    .blurb(
                        "arrival: stamp each buffer when it arrives, on this pipeline's clock. \
                         owner: keep the owner's timestamps, for a reader that places them itself",
                    )
                    .default_value(Some("arrival"))
                    .build(),
            ]
        });
        P.as_ref()
    }

    fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
        let v = value.get::<Option<String>>().unwrap().unwrap_or_default();
        match pspec.name() {
            "bus-name" => *self.name.lock().unwrap() = v,
            "timestamps" => {
                let owner = v == "owner";
                self.owner_time.store(owner, SeqCst);
                self.obj().set_do_timestamp(!owner);
            }
            _ => *self.dir.lock().unwrap() = v,
        }
    }

    fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
        match pspec.name() {
            "bus-name" => self.name.lock().unwrap().to_value(),
            "timestamps" => {
                if self.owner_time.load(SeqCst) { "owner" } else { "arrival" }.to_value()
            }
            _ => self.dir.lock().unwrap().to_value(),
        }
    }

    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.set_live(true);
        obj.set_format(gst::Format::Time);
        obj.set_do_timestamp(true);
    }
}

impl GstObjectImpl for BusSrc {}

impl ElementImpl for BusSrc {
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        static M: LazyLock<gst::subclass::ElementMetadata> = LazyLock::new(|| {
            gst::subclass::ElementMetadata::new(
                "GodwinMix frame bus source",
                "Source/Video",
                "Reads decoded frames from the frame bus with no copy",
                "GodwinMix",
            )
        });
        Some(&M)
    }

    fn pad_templates() -> &'static [gst::PadTemplate] {
        static T: LazyLock<Vec<gst::PadTemplate>> = LazyLock::new(|| {
            vec![gst::PadTemplate::new(
                "src",
                gst::PadDirection::Src,
                gst::PadPresence::Always,
                &template_caps(),
            )
            .unwrap()]
        });
        T.as_ref()
    }
}
