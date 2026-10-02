//! The key while it runs: the settings in force, the table built from them,
//! and what happens to each frame.
//!
//! Called from a pad probe on the slot's own streaming thread, which is the
//! thread of the queue above the filter, never the compositor's. A frame is
//! one table lookup per block and one write per pixel the matte keeps; a
//! settings change builds a new table on the control thread and swaps it in.

use super::frame::{self, Region, Scratch, I420};
use super::guess::{self, Guess};
use super::lut::Lut;
use super::params::{Colour, Settings};
use crate::overlay::picture::{Area, Picture};
use crate::plugin::filter::BoardHook;
use gstreamer as gst;
use gstreamer_video as gst_video;
use parking_lot::Mutex;
use std::sync::Arc;

/// How often a key with no colour yet looks for one, after the first second.
const LOOK_EVERY: u32 = 15;

pub struct Keyer {
    pub hook: BoardHook,
    state: Mutex<State>,
}

struct State {
    settings: Settings,
    lut: Arc<Lut>,
    /// The colour in force, as Y, U and V: given, or found.
    key: Option<(u8, u8, u8)>,
    found: Option<Guess>,
    frames: u32,
    scratch: Scratch,
    info: Option<(gst::Caps, gst_video::VideoInfo)>,
}

impl Keyer {
    pub fn new(settings: Settings) -> Arc<Keyer> {
        let state = State {
            settings,
            lut: Arc::new(Lut::opaque()),
            key: None,
            found: None,
            frames: 0,
            scratch: Scratch::default(),
            info: None,
        };
        let keyer = Arc::new(Keyer { hook: BoardHook::new(), state: Mutex::new(state) });
        keyer.set(settings);
        keyer
    }

    /// Put new settings in force. The table is built before the lock is
    /// taken, so the frame in flight waits for a swap and nothing else.
    pub fn set(&self, settings: Settings) {
        let (old, found) = {
            let st = self.state.lock();
            (st.settings, st.found)
        };
        let key = match settings.colour {
            Colour::Rgb(rgb) => Some(super::colour::rgb_to_yuv(rgb)),
            // A guess already made stands while only the numbers change.
            Colour::Auto => found.filter(|_| old.family == settings.family).map(|g| g.yuv),
        };
        let lut = Arc::new(match key {
            Some((_, u, v)) => Lut::build((u, v), &settings),
            None => Lut::opaque(),
        });
        let mut st = self.state.lock();
        st.settings = settings;
        st.key = key;
        st.lut = lut;
        if key.is_none() {
            st.found = None;
            st.frames = 0;
        }
    }

    /// One frame on its way to the compositor pad.
    pub fn on_buffer(&self, pad: &gst::Pad, info: &mut gst::PadProbeInfo) -> gst::PadProbeReturn {
        let Some(gst::PadProbeData::Buffer(buffer)) = info.data.as_mut() else {
            return gst::PadProbeReturn::Ok;
        };
        let mut st = self.state.lock();
        let Some(vinfo) = st.video_info(pad) else { return gst::PadProbeReturn::Ok };
        if st.key.is_none() {
            st.look(buffer, &vinfo);
        }
        if self.hook.layer.active() {
            if self.hook.seen() {
                if let Some(p) = st.picture(buffer, &vinfo) {
                    self.hook.layer.set_picture(Some(Arc::new(p)));
                }
            }
            *buffer = gap(buffer);
            return gst::PadProbeReturn::Ok;
        }
        if st.key.is_some() {
            st.flatten(buffer.make_mut(), &vinfo);
        }
        gst::PadProbeReturn::Ok
    }
}

impl State {
    fn video_info(&mut self, pad: &gst::Pad) -> Option<gst_video::VideoInfo> {
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
    fn look(&mut self, buffer: &gst::BufferRef, info: &gst_video::VideoInfo) {
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

    /// The keyed picture of the area the matte keeps.
    fn picture(&mut self, buffer: &gst::BufferRef, info: &gst_video::VideoInfo) -> Option<Picture> {
        let f = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, info).ok()?;
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
        let mut out = vec![0u8; r.w * r.h * 4];
        let lut = self.lut.clone();
        frame::key(&src, r, &lut, self.settings.feather, &mut self.scratch, &mut out);
        let whole = r == Region { x: 0, y: 0, w, h };
        let within = (!whole).then_some(Area { x: r.x as u32, y: r.y as u32, w: r.w as u32, h: r.h as u32 });
        let mut pic = Picture::from_ayuv(out, r.w as u32, r.h as u32, (w as u32, h as u32));
        pic.within = within;
        Some(pic)
    }

    fn flatten(&mut self, buffer: &mut gst::BufferRef, info: &gst_video::VideoInfo) {
        let Ok(mut f) = gst_video::VideoFrameRef::from_buffer_ref_writable(buffer, info) else { return };
        let s = info.stride();
        let strides = [s[0] as usize, s[1] as usize, s[2] as usize];
        let size = (info.width() as usize, info.height() as usize);
        let [y, u, v, _] = f.planes_data_mut();
        frame::flatten(y, u, v, strides, size, &self.lut, &self.settings.matte);
    }
}

/// An empty buffer flagged as a gap, in place of the frame the board draws.
///
/// Dropping the frame instead would leave the compositor pad with nothing
/// for its time: a pad that has had buffers and then stops is waited for, a
/// whole upstream latency of programme at a time. A gap says the time has
/// passed with nothing to draw, so the compositor neither waits for this pad
/// nor goes on drawing the last frame it was given.
fn gap(frame: &gst::BufferRef) -> gst::Buffer {
    let mut out = gst::Buffer::new();
    {
        let b = out.get_mut().expect("a new buffer is writable");
        b.set_pts(frame.pts());
        b.set_dts(frame.dts());
        b.set_duration(frame.duration());
        b.set_flags(gst::BufferFlags::GAP | gst::BufferFlags::DROPPABLE);
    }
    out
}
