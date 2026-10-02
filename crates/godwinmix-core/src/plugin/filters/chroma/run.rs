//! The key while it runs: the settings in force, the table built from them,
//! and what happens to each frame.
//!
//! Called from a pad probe on the slot's own streaming thread, which is the
//! thread of the queue above the filter, never the compositor's. A frame is
//! one table lookup per block and one write per pixel the matte keeps; a
//! settings change builds a new table on the control thread and swaps it in.

use super::lut::Lut;
use super::state::{gap, Handed, State};
use super::params::{Colour, Settings};
use crate::plugin::filter::BoardHook;
use gstreamer as gst;
use parking_lot::Mutex;
use std::sync::Arc;

pub struct Keyer {
    pub hook: BoardHook,
    state: Mutex<State>,
}


impl Keyer {
    pub fn new(settings: Settings) -> Arc<Keyer> {
        let state = State {
            settings,
            lut: Arc::new(Lut::opaque()),
            key: None,
            found: None,
            frames: 0,
            handed: Handed::default(),
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
            let frame = buffer.clone();
            *buffer = gap(&frame);
            if self.hook.seen() {
                if let Some(p) = st.picture(&frame, &vinfo) {
                    self.hook.layer.set_picture(Some(Arc::new(p)));
                }
            }
            return gst::PadProbeReturn::Ok;
        }
        if st.key.is_some() {
            st.flatten(buffer.make_mut(), &vinfo);
        }
        gst::PadProbeReturn::Ok
    }
}
