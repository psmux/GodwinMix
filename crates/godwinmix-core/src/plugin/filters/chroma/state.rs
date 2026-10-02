//! What the key holds between frames, and what it does to one: look for
//! the screen colour, decide the matte for the board, or flatten the key into
//! the frame where there is no board.

use super::frame::{self, Region, I420};
use super::guess::{self, Guess};
pub(super) use super::handed::{gap, Handed};
use super::lut::Lut;
use super::params::Settings;
use crate::overlay::keyed::Keyed;
use crate::overlay::picture::{Area, Picture};
use gstreamer as gst;
use gstreamer_video as gst_video;
use std::sync::Arc;

/// How often a key with no colour yet looks for one, after the first second.
const LOOK_EVERY: u32 = 15;

pub(super) struct State {
    pub(super) settings: Settings,
    pub(super) lut: Arc<Lut>,
    /// The colour in force, as Y, U and V: given, or found.
    pub(super) key: Option<(u8, u8, u8)>,
    pub(super) found: Option<Guess>,
    pub(super) frames: u32,
    /// Keyed frames handed to the board, newest last.
    pub(super) handed: Handed,
    pub(super) info: Option<(gst::Caps, gst_video::VideoInfo)>,
}

impl State {
    pub(super) fn video_info(&mut self, pad: &gst::Pad) -> Option<gst_video::VideoInfo> {
        use gstreamer::prelude::*;
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

    /// Look for the screen colour in this frame: every frame for the first
    /// second, then every `LOOK_EVERY`th, so a camera that starts on black
    /// is keyed as soon as it shows the screen.
    pub(super) fn look(&mut self, buffer: &gst::BufferRef, info: &gst_video::VideoInfo) {
        self.frames += 1;
        if self.frames > 30 && self.frames % LOOK_EVERY != 0 {
            return;
        }
        let Ok(f) = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, info) else { return };
        let (Ok(y), Ok(u), Ok(v)) = (f.plane_data(0), f.plane_data(1), f.plane_data(2)) else { return };
        let s = info.stride();
        let (cw, ch) = (info.width() as usize / 2, info.height() as usize / 2);
        let samples = (0..ch).step_by(4).flat_map(|cy| (0..cw).step_by(4).map(move |cx| (cx, cy))).map(|(cx, cy)| {
            (y[cy * 2 * s[0] as usize + cx * 2], u[cy * s[1] as usize + cx], v[cy * s[2] as usize + cx])
        });
        if let Some(g) = guess::dominant(samples, self.settings.family) {
            tracing::info!(colour = %super::colour::to_hex(g.rgb), share = g.share, "the key found its colour");
            self.found = Some(g);
            self.key = Some(g.yuv);
            self.lut = Arc::new(Lut::build((g.yuv.1, g.yuv.2), &self.settings));
        }
    }

    /// The frame and the key's decision for it, for the board to draw. The
    /// frame itself is kept by reference, not copied.
    pub(super) fn picture(&mut self, frame: &gst::Buffer, info: &gst_video::VideoInfo) -> Option<Picture> {
        let f = gst_video::VideoFrameRef::from_buffer_ref_readable(frame.as_ref(), info).ok()?;
        let s = info.stride();
        let (w, h) = (info.width() as usize, info.height() as usize);
        let src = I420 {
            y: f.plane_data(0).ok()?,
            u: f.plane_data(1).ok()?,
            v: f.plane_data(2).ok()?,
            strides: [s[0] as usize, s[1] as usize, s[2] as usize],
            width: w,
            height: h,
        };
        let r = Region::of(w, h, &self.settings.matte);
        let (mut alpha, mut chroma) = self.reuse();
        frame::blocks(&src, r, &self.lut, self.settings.feather, &mut alpha, &mut chroma);
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
            within,
            keyed: Some(keyed),
        })
    }

    /// The block arrays of a keyed frame the board has finished with, so a
    /// frame does not allocate its own. The layer holds the newest and the
    /// board may hold one more while it draws; anything older is free.
    fn reuse(&mut self) -> (Vec<u8>, Vec<[u8; 2]>) {
        self.handed.truncate_front(3);
        let free = self.handed.iter().position(|k| Arc::strong_count(k) == 1);
        match free.map(|i| self.handed.remove(i)).and_then(|k| Arc::try_unwrap(k).ok()) {
            Some(k) => (k.alpha, k.chroma),
            None => (Vec::new(), Vec::new()),
        }
    }

    pub(super) fn flatten(&mut self, buffer: &mut gst::BufferRef, info: &gst_video::VideoInfo) {
        let Ok(mut f) = gst_video::VideoFrameRef::from_buffer_ref_writable(buffer, info) else { return };
        let s = info.stride();
        let strides = [s[0] as usize, s[1] as usize, s[2] as usize];
        let size = (info.width() as usize, info.height() as usize);
        let [y, u, v, _] = f.planes_data_mut();
        frame::flatten(y, u, v, strides, size, &self.lut, &self.settings.matte);
    }
}
