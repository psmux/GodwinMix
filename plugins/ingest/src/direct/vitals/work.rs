//! The workers: a few threads that decode for every show.
//!
//! A job is one keyframe (or one decoded frame from a decode that already
//! exists), or a short burst of sound. `pool.rs` is the queue in front of
//! them, which never makes a tap wait.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use gstreamer as gst;

use super::chain::Chain;
use super::pool::Queue;
use super::show::{lock, Show, Thumb};
use super::{measure, now_ms};

pub enum Job {
    /// A keyframe, or a raw frame, with its caps. `jpeg`: make a thumbnail.
    Picture { show: Arc<Show>, caps: gst::Caps, buffer: gst::Buffer, jpeg: bool },
    /// A few sound frames in a row; the first only primes the decoder.
    Sound { show: Arc<Show>, caps: gst::Caps, buffers: Vec<gst::Buffer> },
}

impl Job {
    /// Which waiting slot it takes: one picture and one burst per show.
    pub fn key(&self) -> (String, u8) {
        match self {
            Job::Picture { show, .. } => (show.id.clone(), 0),
            Job::Sound { show, .. } => (show.id.clone(), 1),
        }
    }
}

/// One worker's chains, built on first use, one per kind of input, and the
/// kinds whose hardware decoder has refused a keyframe and are decoded on
/// the CPU from then on.
#[derive(Default)]
struct Chains {
    built: HashMap<String, Chain>,
    cpu: HashSet<String>,
}

impl Chains {
    fn get(&mut self, caps: &gst::Caps) -> Option<&mut Chain> {
        let name = caps.structure(0)?.name().to_string();
        if !self.built.contains_key(&name) {
            let built = measure::chain_for(&name, self.cpu.contains(&name))?;
            self.built.insert(name.clone(), built);
        }
        self.built.get_mut(&name)
    }

    /// A chain gave nothing back: build it again, on the CPU this time.
    fn refused(&mut self, caps: &gst::Caps) {
        if let Some(s) = caps.structure(0) {
            self.built.remove(s.name().as_str());
            self.cpu.insert(s.name().to_string());
        }
    }
}

/// One worker, until the queue closes.
pub fn run(queue: &Queue) {
    let mut chains = Chains::default();
    while let Some(job) = queue.next() {
        match job {
            Job::Picture { show, caps, buffer, jpeg } => picture(&mut chains, &show, &caps, buffer, jpeg),
            Job::Sound { show, caps, buffers } => sound(&mut chains, &show, &caps, buffers),
        }
        queue.done.fetch_add(1, Ordering::Relaxed);
    }
}

fn picture(chains: &mut Chains, show: &Show, caps: &gst::Caps, buffer: gst::Buffer, jpeg: bool) {
    let Some(c) = chains.get(caps) else { return };
    let Some(sample) = c.run(caps, vec![buffer]).pop() else {
        chains.refused(caps);
        return;
    };
    let now = now_ms();
    if let Some(luma) = measure::luma(&sample) {
        show.picture(luma, now);
    }
    if !jpeg {
        return;
    }
    let (Some(scaled_caps), Some(buffer)) = (sample.caps().map(|c| c.to_owned()), sample.buffer_owned()) else { return };
    // The asked width, and the height that keeps the picture's shape.
    let (w, h) = measure::size(&scaled_caps);
    let wide = show.jpeg_width.load(Ordering::Relaxed);
    let high = (u64::from(h) * u64::from(wide) / u64::from(w.max(1))).max(2) as u32 & !1;
    let Some(enc) = chains.get(&gst::Caps::new_empty_simple(format!("jpeg/{wide}x{high}"))) else { return };
    if let Some(out) = enc.run(&scaled_caps, vec![buffer]).pop() {
        let (width, height) = out.caps().map(measure::size).unwrap_or_default();
        let bytes = out.buffer().and_then(|b| b.map_readable().ok().map(|m| Arc::<[u8]>::from(m.as_slice())));
        if let Some(jpeg) = bytes {
            *lock(&show.thumb) = Some(Thumb { jpeg, width, height, at_ms: now });
        }
    }
}

fn sound(chains: &mut Chains, show: &Show, caps: &gst::Caps, buffers: Vec<gst::Buffer>) {
    let Some(c) = chains.get(caps) else { return };
    let samples = c.run(caps, buffers);
    // The first frame out of a decoder that has just been flushed is made
    // without the frame before it, so it is not measured.
    let peak = samples.iter().skip(1).filter_map(measure::peak).reduce(f64::max);
    if let Some(peak) = peak {
        lock(&show.judge).sound(measure::to_db(peak), now_ms());
    }
}
