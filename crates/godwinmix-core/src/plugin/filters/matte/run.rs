//! The cutout on the frame's own thread: hand the frame to the worker, lay
//! the newest mask over it, and give the board the result.
//!
//! What runs here is the cheap half: a reference handed over, a table lookup
//! per block, and the same handing on to the board the chroma key does. The
//! model runs on the worker's thread (`worker`), and a frame never waits for
//! it.

use super::blocks;
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

impl State {
    fn video_info(&mut self, pad: &gst::Pad) -> Option<gst_video::VideoInfo> {
        let caps = pad.current_caps()?;
        if let Some((have, info)) = &self.info {
            if have == &caps {
                return Some(info.clone());
            }
        }
        let info = gst_video::VideoInfo::from_caps(&caps).ok()?;
        if info.format() != gst_video::VideoFormat::I420 {
            return None;
        }
        self.info = Some((caps, info.clone()));
        Some(info)
    }

    fn alpha_into(&self, size: (usize, usize), r: Region, mask: Option<&super::worker::Mask>, out: &mut Vec<u8>) {
        match mask {
            Some(m) => blocks::alpha(&m.data, (m.width, m.height), size, r, &self.curve, out),
            None => blocks::alpha(&[], (0, 0), size, r, &self.curve, out),
        }
        if self.settings.feather > 0 {
            crate::plugin::filters::chroma::feather::soften(out, r.w / 2, r.h / 2, self.settings.feather.div_ceil(2) as usize);
        }
    }

    fn picture(&mut self, frame: &gst::Buffer, info: &gst_video::VideoInfo, mask: Option<&super::worker::Mask>) -> Option<Picture> {
        let f = gst_video::VideoFrameRef::from_buffer_ref_readable(frame.as_ref(), info).ok()?;
        let s = info.stride();
        let strides = [s[0] as usize, s[1] as usize, s[2] as usize];
        let (w, h) = (info.width() as usize, info.height() as usize);
        let r = Region::of(w, h, &self.settings.matte);
        let (mut alpha, mut chroma) = self.reuse();
        self.alpha_into((w, h), r, mask, &mut alpha);
        blocks::chroma(f.plane_data(1).ok()?, f.plane_data(2).ok()?, strides, r, &mut chroma);
        let keyed = Arc::new(Keyed { frame: frame.clone(), info: info.clone(), region: (r.x, r.y, r.w, r.h), alpha, chroma });
        self.handed.push(keyed.clone());
        let whole = r == Region { x: 0, y: 0, w, h };
        let within = (!whole).then_some(Area { x: r.x as u32, y: r.y as u32, w: r.w as u32, h: r.h as u32 });
        Some(Picture {
            buffer: gst::Buffer::new(),
            width: r.w as u32,
            height: r.h as u32,
            stride: 0,
            natural: (w as u32, h as u32),
            content: None,
            within,
            keyed: Some(keyed),
        })
    }

    /// The block arrays of a picture the board has finished with.
    fn reuse(&mut self) -> (Vec<u8>, Vec<[u8; 2]>) {
        self.handed.truncate_front(3);
        let free = self.handed.iter().position(|k| Arc::strong_count(k) == 1);
        match free.map(|i| self.handed.remove(i)).and_then(|k| Arc::try_unwrap(k).ok()) {
            Some(k) => (k.alpha, k.chroma),
            None => (Vec::new(), Vec::new()),
        }
    }

    fn flatten(&mut self, buffer: &mut gst::BufferRef, info: &gst_video::VideoInfo, mask: Option<&super::worker::Mask>) {
        let (w, h) = (info.width() as usize, info.height() as usize);
        let r = Region::of(w, h, &self.settings.matte);
        let mut alpha = std::mem::take(&mut self.flat);
        self.alpha_into((w, h), r, mask, &mut alpha);
        if let Ok(mut f) = gst_video::VideoFrameRef::from_buffer_ref_writable(buffer, info) {
            let s = info.stride();
            let strides = [s[0] as usize, s[1] as usize, s[2] as usize];
            let [y, u, v, _] = f.planes_data_mut();
            blocks::flatten(y, u, v, strides, (w, h), &alpha, r);
        }
        self.flat = alpha;
    }
}
