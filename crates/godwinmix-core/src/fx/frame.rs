//! A transition worked on whole pictures: a luma matte wipe or a shader.
//!
//! Both need the scene going away and the scene coming on as two pictures,
//! and the compositor makes one. So the compositor cuts to the new scene at
//! the start of the window, and this pass draws the old one back over it,
//! pixel by pixel, by the matte or the shader.
//!
//! The old picture comes from one of two places. When the scene going away
//! was one source filling the canvas, which is the camera to camera case,
//! its pad is still being fed for the length of the window, so the pass
//! reads that source's newest frame and the old picture keeps moving
//! (`Outgoing::Live`). Otherwise the pass keeps a copy of the last programme
//! frames before the window opens, which the mixer starts two frames after
//! the take for exactly this, and the old picture holds still while it is
//! wiped away (`Outgoing::Held`).

use super::Mix;
use crate::mixer::transition::Easing;
use crate::overlay::blend::Planes;
use crate::overlay::pass::Pass;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video as gst_video;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// A picture to read from: three planes of an I420 frame the canvas size.
pub struct Pic<'a> {
    pub y: &'a [u8],
    pub u: &'a [u8],
    pub v: &'a [u8],
    pub strides: [usize; 3],
}

/// A copy of one programme frame.
#[derive(Default)]
pub struct Held {
    planes: [Vec<u8>; 3],
    strides: [usize; 3],
}

impl Held {
    fn keep(&mut self, f: &Planes<'_>) {
        for (i, src) in [&*f.y, &*f.u, &*f.v].into_iter().enumerate() {
            self.planes[i].clear();
            self.planes[i].extend_from_slice(src);
        }
        self.strides = f.strides;
    }

    fn pic(&self) -> Option<Pic<'_>> {
        (!self.planes[0].is_empty()).then(|| Pic { y: &self.planes[0], u: &self.planes[1], v: &self.planes[2], strides: self.strides })
    }
}

/// Where the old picture comes from.
pub enum Outgoing {
    /// The newest frame on the old scene's one pad, and that pad's layout.
    Live { pad: gst::Pad, probe: Option<gst::PadProbeId>, last: std::sync::Arc<Mutex<Option<gst::Buffer>>>, info: gst_video::VideoInfo },
    Held,
}

impl Outgoing {
    /// Watch `pad` for its newest frame, if its frames are whole canvas
    /// pictures. `None` when they are not, and the caller holds a frame.
    pub fn live(pad: &gst::Pad, canvas: (i32, i32)) -> Option<Outgoing> {
        let caps = pad.current_caps()?;
        let info = gst_video::VideoInfo::from_caps(&caps).ok()?;
        if (info.width() as i32, info.height() as i32) != canvas || info.format() != gst_video::VideoFormat::I420 {
            return None;
        }
        let last = std::sync::Arc::new(Mutex::new(None));
        let keep = last.clone();
        let probe = pad.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
            if let Some(gst::PadProbeData::Buffer(b)) = &info.data {
                *keep.lock() = Some(b.clone());
            }
            gst::PadProbeReturn::Ok
        });
        Some(Outgoing::Live { pad: pad.clone(), probe, last, info })
    }
}

impl Drop for Outgoing {
    fn drop(&mut self) {
        if let Outgoing::Live { pad, probe, .. } = self {
            if let Some(id) = probe.take() {
                pad.remove_probe(id);
            }
        }
    }
}

pub struct FramePass {
    name: String,
    start: u64,
    end: u64,
    easing: Easing,
    outgoing: Outgoing,
    held: Mutex<Held>,
    mix: Box<dyn Mix>,
    done: AtomicBool,
}

impl FramePass {
    pub fn new(name: &str, window: (u64, u64), easing: Easing, outgoing: Outgoing, mix: Box<dyn Mix>) -> FramePass {
        let (start, end) = window;
        FramePass { name: name.to_string(), start, end, easing, outgoing, held: Mutex::default(), mix, done: AtomicBool::new(false) }
    }

    fn paint_live(&self, frame: &mut Planes<'_>, t: f64) -> bool {
        let Outgoing::Live { last, info, .. } = &self.outgoing else { return false };
        let Some(buffer) = last.lock().clone() else { return false };
        let Ok(f) = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer.as_ref(), info) else { return false };
        let (Ok(y), Ok(u), Ok(v)) = (f.plane_data(0), f.plane_data(1), f.plane_data(2)) else { return false };
        let s = info.stride();
        let pic = Pic { y, u, v, strides: [s[0] as usize, s[1] as usize, s[2] as usize] };
        self.mix.mix(&pic, frame, t);
        true
    }
}

impl Pass for FramePass {
    fn paint(&self, frame: &mut Planes<'_>, now: u64) {
        if now < self.start {
            self.held.lock().keep(frame);
            return;
        }
        if now >= self.end {
            self.done.store(true, Ordering::Release);
            return;
        }
        let t = self.easing.at((now - self.start) as f64 / (self.end - self.start).max(1) as f64);
        if self.paint_live(frame, t) {
            return;
        }
        let held = self.held.lock();
        if let Some(pic) = held.pic() {
            self.mix.mix(&pic, frame, t);
        }
    }

    fn finished(&self) -> bool {
        self.done.load(Ordering::Acquire)
    }

    fn name(&self) -> &str {
        &self.name
    }
}
