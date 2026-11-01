//! The cutout on the frame's own thread: hand the frame to the worker, lay
//! the newest mask over it, and give the board the result.
//!
//! What runs here is the cheap half: a reference handed over, a table lookup
//! per block, and the same handing on to the board the chroma key does. The
//! model runs on the worker's thread (`worker`), and a frame never waits for
//! it.

use super::blocks;
#[path = "picture.rs"]
mod picture;
use super::params::Settings;
use super::worker::Worker;
use crate::overlay::keyed::Keyed;
use crate::overlay::picture::{Area, Picture};
use crate::plugin::filter::BoardHook;
use crate::plugin::filters::chroma::frame::Region;
use crate::plugin::filters::chroma::handed::{gap, Handed};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video as gst_video;
use parking_lot::Mutex;
use std::sync::Arc;

pub struct Cutter {
    pub hook: BoardHook,
    worker: Worker,
    state: Mutex<State>,
}

struct State {
    settings: Settings,
    curve: [u8; 256],
    handed: Handed,
    info: Option<(gst::Caps, gst_video::VideoInfo)>,
    flat: Vec<u8>,
}

impl Cutter {
    pub fn new(settings: Settings, hook: BoardHook) -> Arc<Cutter> {
        let state = State {
            curve: settings.curve(),
            settings: settings.clone(),
            handed: Handed::default(),
            info: None,
            flat: Vec::new(),
        };
        Arc::new(Cutter { hook, worker: Worker::start(settings), state: Mutex::new(state) })
    }

    pub fn set(&self, settings: Settings) {
        self.worker.set(settings.clone());
        let mut st = self.state.lock();
        st.curve = settings.curve();
        st.settings = settings;
    }

    /// One frame on its way to the compositor pad.
    pub fn on_buffer(&self, pad: &gst::Pad, info: &mut gst::PadProbeInfo) -> gst::PadProbeReturn {
        let Some(gst::PadProbeData::Buffer(buffer)) = info.data.as_mut() else {
            return gst::PadProbeReturn::Ok;
        };
        let mut st = self.state.lock();
        let Some(vinfo) = st.video_info(pad) else { return gst::PadProbeReturn::Ok };
        let frame = buffer.clone();
        self.worker.offer(&frame, &vinfo);
        let mask = self.worker.latest();
        if self.hook.layer.active() {
            *buffer = gap(&frame);
            if self.hook.seen() {
                if let Some(p) = st.picture(&frame, &vinfo, mask.as_deref()) {
                    self.hook.layer.set_picture(Some(Arc::new(p)));
                }
            }
            return gst::PadProbeReturn::Ok;
        }
        if mask.is_some() {
            st.flatten(buffer.make_mut(), &vinfo, mask.as_deref());
        }
        gst::PadProbeReturn::Ok
    }
}
