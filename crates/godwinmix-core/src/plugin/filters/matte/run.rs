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
use super::worker::{share, Mask, Worker};
use crate::overlay::keyed::Keyed;
use crate::overlay::picture::{Area, Picture};
use crate::plugin::filter::BoardHook;
use crate::plugin::filters::chroma::frame::Region;
use crate::plugin::filters::chroma::handed::{gap, Handed};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video as gst_video;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct Cutter {
    pub hook: BoardHook,
    /// Shared with every other cutout on the same picture asking the same
    /// masks (`share`), and swapped for another when a change of settings
    /// or of picture means this one no longer does.
    worker: Mutex<Arc<Worker>>,
    /// The last mask of the worker this cutout left, drawn until the new one
    /// answers, so a cutout on air is never without a mask while a model
    /// loads.
    held: Mutex<Option<Arc<Mask>>>,
    /// Whose picture, as `filter::share_key` says it, and whether this copy
    /// leads. See `Filter::share_as`.
    picture: Mutex<Option<String>>,
    lead: bool,
    /// Tells this cutout's offers from another lead's on the same worker.
    me: u64,
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
    pub fn new(settings: Settings, hook: BoardHook, picture: Option<String>, lead: bool) -> Arc<Cutter> {
        let state = State {
            curve: settings.curve(),
            settings: settings.clone(),
            handed: Handed::default(),
            info: None,
            flat: Vec::new(),
        };
        static MADE: AtomicU64 = AtomicU64::new(0);
        let worker = Mutex::new(share::worker_for(picture.as_deref(), &settings));
        let me = MADE.fetch_add(1, Ordering::Relaxed);
        let picture = Mutex::new(picture);
        Arc::new(Cutter { hook, worker, held: Mutex::new(None), picture, lead, me, state: Mutex::new(state) })
    }

    /// New settings. The edge and the feather are this cutout's own; a
    /// change to what the model is asked moves this cutout to a worker that
    /// answers it (`share::settle`), so two cutouts on one picture end on one
    /// model whichever of them is told first.
    pub fn set(&self, settings: Settings) {
        let key = self.picture.lock().clone();
        let current = self.worker.lock().clone();
        match share::settle(key.as_deref(), &current, &settings) {
            Some(next) => self.move_to(next),
            None => current.set(settings.clone()),
        }
        let mut st = self.state.lock();
        st.curve = settings.curve();
        st.settings = settings;
    }

    /// The picture this cutout works on has changed (the item's crop or turn).
    pub fn reshare(&self, key: &str) {
        if self.picture.lock().as_deref() == Some(key) {
            return;
        }
        *self.picture.lock() = Some(key.to_string());
        let settings = self.state.lock().settings.clone();
        // Found or started before the worker's lock is taken: the frame's
        // thread takes that lock, and starting a thread is not quick.
        self.move_to(share::worker_for(Some(key), &settings));
    }

    fn move_to(&self, next: Arc<Worker>) {
        let old = std::mem::replace(&mut *self.worker.lock(), next);
        let mut held = self.held.lock();
        *held = old.latest().or_else(|| held.take());
    }

    fn mask(&self, frame: &gst::Buffer, vinfo: &gst_video::VideoInfo, wanted: bool) -> Option<Arc<Mask>> {
        // Held, not cloned: a clone would count as another cutout sharing it.
        let worker = self.worker.lock();
        if wanted {
            worker.offer_as(frame, vinfo, self.lead, self.me);
        }
        let latest = worker.latest();
        drop(worker);
        latest.or_else(|| self.held.lock().clone())
    }

    /// One frame on its way to the compositor pad.
    pub fn on_buffer(&self, pad: &gst::Pad, info: &mut gst::PadProbeInfo) -> gst::PadProbeReturn {
        let Some(gst::PadProbeData::Buffer(buffer)) = info.data.as_mut() else {
            return gst::PadProbeReturn::Ok;
        };
        let mut st = self.state.lock();
        let Some(vinfo) = st.video_info(pad) else { return gst::PadProbeReturn::Ok };
        let frame = buffer.clone();
        // A cutout drawn by the board on pads nobody can see (an item off
        // air whose slot is kept, a scene armed but faded out) feeds the model
        // nothing: it used to run flat out on a camera that was not on screen.
        let wanted = !self.hook.layer.active() || self.hook.seen();
        let mask = self.mask(&frame, &vinfo, wanted);
        if self.hook.layer.active() {
            *buffer = gap(&frame);
            if self.hook.seen() {
                if let Some(p) = st.picture(&frame, &vinfo, mask.as_deref()) {
                    self.hook.layer.set_picture(Some(Arc::new(p)));
                }
            }
            return gst::PadProbeReturn::Ok;
        }
        // Drawn by the compositor itself. Before the first mask that would be
        // the whole camera, which went to air over the set for a second or
        // two after a restart, before the board took the item; a gap draws
        // nothing until the model answers.
        match mask {
            Some(m) => st.flatten(buffer.make_mut(), &vinfo, Some(&m)),
            None => *buffer = gap(&frame),
        }
        gst::PadProbeReturn::Ok
    }
}
