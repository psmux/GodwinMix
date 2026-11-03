//! The cutout's own thread: the model, fed the newest frame, answering the
//! newest mask.
//!
//! The frame's thread never waits for it. It hands over a reference to the
//! frame it has (`offer`, no copy) and reads whichever mask was finished last
//! (`latest`). A model slower than the frame rate simply answers for fewer
//! frames; the picture keeps its frame rate and the edge follows a frame or
//! two behind, which is what every camera cutout does.
//!
//! The session is opened on this thread too, on the first frame: a model can
//! take a second to load and a GPU longer to prepare, and none of that may
//! happen where a frame is waiting. A session a stopped worker left behind is
//! taken before a new one is opened (`warm`), so arming a cutout scene again
//! does not load the model again.

use super::model::{self, Spec};
use super::params::Settings;
use super::runtime::{self, Ran};
use super::sample;
#[path = "inference.rs"]
mod inference;
use inference::{infer, prepare, wait_for_change, Open};
use gstreamer as gst;
use gstreamer_video as gst_video;
use parking_lot::{Condvar, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
#[path = "share.rs"]
pub mod share;
#[path = "warm.rs"]
pub mod warm;

/// One mask, a byte per pixel at the model's size.
pub struct Mask {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

struct Shared {
    job: Mutex<Option<(gst::Buffer, gst_video::VideoInfo)>>,
    wake: Condvar,
    mask: Mutex<Option<Arc<Mask>>>,
    settings: Mutex<Settings>,
    /// The last reason the model could not start, so it is logged once.
    error: Mutex<Option<String>>,
    stop: AtomicBool,
    /// Which lead last offered a frame, and when. See `share::offer_as`.
    lead_at: Mutex<Option<(u64, Instant)>>,
}

pub struct Worker {
    shared: Arc<Shared>,
}

impl Worker {
    pub fn start(settings: Settings) -> Worker {
        let shared = Arc::new(Shared {
            job: Mutex::new(None),
            wake: Condvar::new(),
            mask: Mutex::new(None),
            settings: Mutex::new(settings),
            error: Mutex::new(None),
            stop: AtomicBool::new(false),
            lead_at: Mutex::new(None),
        });
        let mine = shared.clone();
        let started = std::thread::Builder::new().name("gmx-cutout".into()).spawn(move || run(&mine));
        if let Err(e) = started {
            tracing::error!(error = %e, "no thread for the cutout; the camera is drawn whole");
        }
        Worker { shared }
    }

    /// Hand over the newest frame. One waiting is enough: a newer frame
    /// replaces it. `share::offer_as` is the one that decides whether to.
    fn offer(&self, frame: &gst::Buffer, info: &gst_video::VideoInfo) {
        *self.shared.job.lock() = Some((frame.clone(), info.clone()));
        self.shared.wake.notify_one();
    }

    pub fn latest(&self) -> Option<Arc<Mask>> {
        self.shared.mask.lock().clone()
    }

    pub fn set(&self, settings: Settings) {
        *self.shared.settings.lock() = settings;
    }

}

impl Drop for Worker {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        self.shared.wake.notify_one();
    }
}

fn run(shared: &Shared) {
    let mut open: Option<Open> = None;
    let (mut input, mut out) = (Vec::new(), Vec::new());
    let mut last: Option<Arc<Mask>> = None;
    let mut frames = 0u64;
    // Counted from the first frame it is handed, because a cutout nobody can
    // see is fed nothing and is not getting ready for anybody.
    let mut counted: Option<warm::Counted> = None;
    loop {
        let (frame, info) = {
            let mut job = shared.job.lock();
            while job.is_none() && !shared.stop.load(Ordering::SeqCst) {
                shared.wake.wait(&mut job);
            }
            if shared.stop.load(Ordering::SeqCst) {
                // Left for the next cutout with these settings.
                if let Some(o) = open.take() {
                    warm::put_back(o);
                }
                return;
            }
            job.take().expect("woken with a job")
        };
        let count = counted.get_or_insert_with(warm::Counted::start);
        let settings = shared.settings.lock().clone();
        if !ensure_open(shared, &settings, &mut open, count) {
            continue;
        }
        let o = open.as_mut().expect("opened above");
        let started = Instant::now();
        let Some(answer) = infer(o, &frame, &info, &mut input) else { continue };
        sample::mask(&answer, last.as_deref().map(|m| m.data.as_slice()), settings.steady, &mut out);
        super::components::clean(&mut out, o.spec.width, o.spec.height);
        let mask = Arc::new(Mask { width: o.spec.width, height: o.spec.height, data: std::mem::take(&mut out) });
        // The old mask's buffer, once nobody draws from it, is the next one's.
        if let Some(old) = last.replace(mask.clone()).and_then(|m| Arc::try_unwrap(m).ok()) {
            out = old.data;
        }
        *shared.mask.lock() = Some(mask);
        count.running();
        frames += 1;
        if frames % 300 == 1 {
            let ms = started.elapsed().as_secs_f32() * 1000.0;
            tracing::debug!(model = %o.spec.name, ms, "a frame through the cutout");
        }
    }
}

/// The session for these settings: the one open, a warm one, or a new one.
/// False when it would not open; the worker has then waited for the settings
/// to change (or to stop) and the caller goes round again.
fn ensure_open(shared: &Shared, settings: &Settings, open: &mut Option<Open>, counted: &mut warm::Counted) -> bool {
    let wanted = (settings.quality.clone(), settings.device);
    if open.as_ref().is_some_and(|o| o.for_settings == wanted) {
        return true;
    }
    if let Some(o) = open.take() {
        warm::put_back(o);
        counted.again();
    }
    if let Some(o) = warm::take(&wanted) {
        tracing::info!(model = %o.spec.name, "the cutout is running, on a model already open");
        *open = Some(o);
        return true;
    }
    match prepare(settings) {
        Ok((session, spec, ran)) => {
            tracing::info!(model = %spec.name, device = %ran.label(), "the cutout is running");
            *shared.error.lock() = None;
            *open = Some(Open { session, spec, for_settings: wanted });
            true
        }
        Err(e) => {
            let why = format!("{e:#}");
            let mut last = shared.error.lock();
            if last.as_deref() != Some(&why) {
                tracing::error!(error = %why, "the cutout could not start; the camera is drawn whole");
            }
            *last = Some(why.clone());
            drop(last);
            counted.failed(&why);
            // Asked again only when the settings change: the same failure
            // every frame would be a log a minute long.
            wait_for_change(shared, &wanted);
            counted.retry();
            false
        }
    }
}
