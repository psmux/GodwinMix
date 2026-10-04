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
//! happen where a frame is waiting.

use super::model::{self, Spec};
use super::params::Settings;
use super::runtime::{self, Ran};
use super::sample::{self, Planes};
use gstreamer as gst;
use gstreamer_video as gst_video;
use parking_lot::{Condvar, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

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
        });
        let mine = shared.clone();
        let started = std::thread::Builder::new().name("gmx-cutout".into()).spawn(move || run(&mine));
        if let Err(e) = started {
            tracing::error!(error = %e, "no thread for the cutout; the camera is drawn whole");
        }
        Worker { shared }
    }

    /// Hand over the newest frame. One waiting is enough: a newer frame
    /// replaces it.
    pub fn offer(&self, frame: &gst::Buffer, info: &gst_video::VideoInfo) {
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

/// The session in use and what it was opened for.
struct Open {
    session: ort::session::Session,
    spec: Spec,
    for_settings: (super::params::Quality, super::params::Device),
}

fn run(shared: &Shared) {
    let mut open: Option<Open> = None;
    let (mut input, mut out) = (Vec::new(), Vec::new());
    let mut last: Option<Arc<Mask>> = None;
    let mut frames = 0u64;
    loop {
        let (frame, info) = {
            let mut job = shared.job.lock();
            while job.is_none() && !shared.stop.load(Ordering::SeqCst) {
                shared.wake.wait(&mut job);
            }
            if shared.stop.load(Ordering::SeqCst) {
                return;
            }
            job.take().expect("woken with a job")
        };
        let settings = shared.settings.lock().clone();
        let wanted = (settings.quality.clone(), settings.device);
        if open.as_ref().is_none_or(|o| o.for_settings != wanted) {
            open = None;
            match prepare(&settings) {
                Ok((session, spec, ran)) => {
                    tracing::info!(model = %spec.name, device = %ran.label(), "the cutout is running");
                    *shared.error.lock() = None;
                    open = Some(Open { session, spec, for_settings: wanted });
                }
                Err(e) => {
                    let why = format!("{e:#}");
                    let mut last = shared.error.lock();
                    if last.as_deref() != Some(&why) {
                        tracing::error!(error = %why, "the cutout could not start; the camera is drawn whole");
                    }
                    *last = Some(why);
                    drop(last);
                    // Asked again only when the settings change: the same
                    // failure every frame would be a log a minute long.
                    wait_for_change(shared, &wanted);
                    continue;
                }
            }
        }
        let o = open.as_mut().expect("opened above");
        let started = Instant::now();
        let Some(answer) = infer(o, &frame, &info, &mut input) else { continue };
        sample::mask(&answer, last.as_deref().map(|m| m.data.as_slice()), settings.steady, &mut out);
        let mask = Arc::new(Mask { width: o.spec.width, height: o.spec.height, data: std::mem::take(&mut out) });
        // The old mask's buffer, once nobody draws from it, is the next one's.
        if let Some(old) = last.replace(mask.clone()).and_then(|m| Arc::try_unwrap(m).ok()) {
            out = old.data;
        }
        *shared.mask.lock() = Some(mask);
        frames += 1;
        if frames % 300 == 1 {
            let ms = started.elapsed().as_secs_f32() * 1000.0;
            tracing::debug!(model = %o.spec.name, ms, "a frame through the cutout");
        }
    }
}

fn prepare(settings: &Settings) -> anyhow::Result<(ort::session::Session, Spec, Ran)> {
    let gpu = model::wants_gpu(settings.device) && runtime::has_gpu(settings.device);
    let spec = model::choose(&settings.quality, gpu)?;
    let (session, ran) = runtime::session(&spec, settings.device)?;
    Ok((session, spec, ran))
}

/// One frame through the model. `None` when the frame cannot be read.
fn infer(o: &mut Open, frame: &gst::Buffer, info: &gst_video::VideoInfo, input: &mut Vec<f32>) -> Option<Vec<f32>> {
    let f = gst_video::VideoFrameRef::from_buffer_ref_readable(frame.as_ref(), info).ok()?;
    let s = info.stride();
    let planes = Planes {
        y: f.plane_data(0).ok()?,
        u: f.plane_data(1).ok()?,
        v: f.plane_data(2).ok()?,
        strides: [s[0] as usize, s[1] as usize, s[2] as usize],
        width: info.width() as usize,
        height: info.height() as usize,
    };
    sample::input(&planes, &o.spec, input);
    let shape = [1usize, 3, o.spec.height, o.spec.width];
    let tensor = ort::value::TensorRef::from_array_view((shape, input.as_slice())).ok()?;
    let outputs = match o.session.run(ort::inputs![tensor]) {
        Ok(outputs) => outputs,
        Err(e) => {
            tracing::warn!(error = %e, "the cutout model failed on a frame");
            return None;
        }
    };
    let (_, data) = outputs[0].try_extract_tensor::<f32>().ok()?;
    // A model may answer at another size; only one the size of its input is
    // used, which every model this reads does.
    (data.len() == o.spec.width * o.spec.height).then(|| data.to_vec())
}

fn wait_for_change(shared: &Shared, from: &(super::params::Quality, super::params::Device)) {
    while !shared.stop.load(Ordering::SeqCst) {
        let now = shared.settings.lock().clone();
        if (now.quality.clone(), now.device) != *from {
            return;
        }
        let mut job = shared.job.lock();
        job.take();
        shared.wake.wait_for(&mut job, std::time::Duration::from_secs(1));
    }
}
