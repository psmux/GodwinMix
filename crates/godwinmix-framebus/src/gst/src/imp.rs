use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use glib::prelude::*;
use glib::subclass::prelude::*;
use gst::subclass::prelude::*;
use gst_base::prelude::*;
use gst_base::subclass::base_src::CreateSuccess;
use gst_base::subclass::prelude::*;
use gstreamer as gst;
use gstreamer_base as gst_base;
use gstreamer_video as gst_video;

use crate::gst::{caps_of, template_caps, CAPTURED_CAPS};
use crate::{BusName, Error, Frame, Layout, Registry, Subscriber};

#[derive(Default)]
pub struct BusSrc {
    name: Mutex<String>,
    dir: Mutex<String>,
    sub: Mutex<Option<Subscriber>>,
    caps_for: Mutex<Option<Layout>>,
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

impl BusSrc {
    fn registry(&self) -> Result<(Registry, BusName), Error> {
        let name: BusName = self.name.lock().unwrap().parse()?;
        let dir = self.dir.lock().unwrap().clone();
        let reg = if dir.is_empty() { Registry::from_env()? } else { Registry::new(dir)? };
        Ok((reg, name))
    }

    /// The next frame, connecting first if nobody published when we started.
    fn frame(&self) -> Result<Option<Frame>, Error> {
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

    fn wrap(&self, frame: Frame) -> Result<gst::Buffer, gst::FlowError> {
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
