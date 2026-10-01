//! The workers: a few threads that decode for every show.
//!
//! A job is one keyframe (or one decoded frame from a decode that already
//! exists), or a short burst of sound. The queue in front of the workers is
//! bounded; when it is full a job is dropped and counted, never waited for,
//! so a tap is never slowed by a decode.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};

use gstreamer as gst;

use super::chain::Chain;
use super::show::{lock, Show, Thumb};
use super::{measure, now_ms};

pub enum Job {
    /// A keyframe, or a raw frame, with its caps. `jpeg`: make a thumbnail.
    Picture { show: Arc<Show>, caps: gst::Caps, buffer: gst::Buffer, jpeg: bool },
    /// A few sound frames in a row; the first only primes the decoder.
    Sound { show: Arc<Show>, caps: gst::Caps, buffers: Vec<gst::Buffer> },
}

/// The queue in front of the workers.
pub struct Pool {
    tx: SyncSender<Job>,
    pub skipped: Arc<AtomicU64>,
    pub done: Arc<AtomicU64>,
}

impl Pool {
    pub fn start(workers: usize, depth: usize) -> Pool {
        let (tx, rx) = sync_channel::<Job>(depth);
        let rx = Arc::new(Mutex::new(rx));
        let done = Arc::new(AtomicU64::new(0));
        for n in 0..workers.max(1) {
            let (rx, done) = (rx.clone(), done.clone());
            std::thread::Builder::new()
                .name(format!("vitals-{n}"))
                .spawn(move || run(rx, done))
                .expect("a vitals worker thread");
        }
        Pool { tx, skipped: Arc::default(), done }
    }

    /// Hand a job over, or drop it if every worker is busy.
    pub fn offer(&self, job: Job) {
        if let Err(TrySendError::Full(_)) = self.tx.try_send(job) {
            self.skipped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// One worker: its own chains, built on first use, one per kind of input.
fn run(rx: Arc<Mutex<Receiver<Job>>>, done: Arc<AtomicU64>) {
    let mut chains: HashMap<String, Chain> = HashMap::new();
    loop {
        let job = match lock(&rx).recv() {
            Ok(job) => job,
            Err(_) => return,
        };
        match job {
            Job::Picture { show, caps, buffer, jpeg } => picture(&mut chains, &show, &caps, buffer, jpeg),
            Job::Sound { show, caps, buffers } => sound(&mut chains, &show, &caps, buffers),
        }
        done.fetch_add(1, Ordering::Relaxed);
    }
}

/// The chain for `caps`, built if this worker has none yet.
fn chain<'a>(chains: &'a mut HashMap<String, Chain>, caps: &gst::Caps) -> Option<&'a mut Chain> {
    let name = caps.structure(0)?.name().to_string();
    if !chains.contains_key(&name) {
        let built = measure::chain_for(&name)?;
        chains.insert(name.clone(), built);
    }
    chains.get_mut(&name)
}

fn picture(chains: &mut HashMap<String, Chain>, show: &Show, caps: &gst::Caps, buffer: gst::Buffer, jpeg: bool) {
    let Some(c) = chain(chains, caps) else { return };
    let Some(sample) = c.run(caps, vec![buffer]).pop() else {
        // A decoder that refused once may be stuck: build it again next time.
        chains.retain(|_, _| false);
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
    let Some(enc) = chain(chains, &gst::Caps::new_empty_simple("jpeg")) else { return };
    if let Some(out) = enc.run(&scaled_caps, vec![buffer]).pop() {
        let (width, height) = measure::size(&scaled_caps);
        let bytes = out.buffer().and_then(|b| b.map_readable().ok().map(|m| Arc::<[u8]>::from(m.as_slice())));
        if let Some(jpeg) = bytes {
            *lock(&show.thumb) = Some(Thumb { jpeg, width, height, at_ms: now });
        }
    }
}

fn sound(chains: &mut HashMap<String, Chain>, show: &Show, caps: &gst::Caps, buffers: Vec<gst::Buffer>) {
    let Some(c) = chain(chains, caps) else { return };
    let samples = c.run(caps, buffers);
    // The first frame out of a decoder that has just been flushed is made
    // without the frame before it, so it is not measured.
    let peak = samples.iter().skip(1).filter_map(measure::peak).reduce(f64::max);
    if let Some(peak) = peak {
        lock(&show.judge).sound(measure::to_db(peak), now_ms());
    }
}
