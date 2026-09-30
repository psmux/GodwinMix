use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::{LazyLock, Mutex};

use glib::prelude::*;
use glib::subclass::prelude::*;
use gst::subclass::prelude::*;
use gst_base::prelude::*;
use gst_base::subclass::base_src::CreateSuccess;
use gst_base::subclass::prelude::*;
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
    flushing: AtomicBool,
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
                glib::ParamSpecString::builder("bus-name").nick("Bus name").blurb("camera:<id> or channel:<app>/<stream>").build(),
                glib::ParamSpecString::builder("bus-dir").nick("Bus directory").blurb("The registry directory; empty for GODWINMIX_BUS_DIR or the default").build(),
            ]
        });
        P.as_ref()
    }

    fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
        let v = value.get::<Option<String>>().unwrap().unwrap_or_default();
        match pspec.name() {
            "bus-name" => *self.name.lock().unwrap() = v,
            _ => *self.dir.lock().unwrap() = v,
        }
    }

    fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
        match pspec.name() {
            "bus-name" => self.name.lock().unwrap().to_value(),
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
            vec![gst::PadTemplate::new("src", gst::PadDirection::Src, gst::PadPresence::Always, &template_caps()).unwrap()]
        });
        T.as_ref()
    }
}

impl BaseSrcImpl for BusSrc {
    fn start(&self) -> Result<(), gst::ErrorMessage> {
        self.registry().map_err(|e| gst::error_msg!(gst::ResourceError::Settings, ["{e}"]))?;
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
