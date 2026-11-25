//! Bringing a bus encoder up and taking it down, both off the mixer thread.
//!
//! Up is tail first, then linked to the bus, the order `EncodeChain::attach`
//! uses. Down is the order `encoder::retire` uses: the chain to NULL, tail
//! first, then the bus tee's pad given back, then the elements out of the
//! pipeline. The pad goes after NULL because giving it back takes its stream
//! lock, which a push stuck in the head queue holds until the queue stops.
//!
//! The two never run at once on one chain. A stop that finds the chain still
//! coming up only says so; the thread bringing it up sees that when it is done
//! and takes it down itself, so no element is brought up after it was taken
//! down or out of the pipeline.

use super::EncoderFailed;
use crate::buses::{AudioCall, Notify};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;

/// Where a chain is, between the thread bringing it up and the mixer.
#[derive(Default)]
pub struct Joined(Mutex<Slot>);

#[derive(Default)]
struct Slot {
    /// The bus tee's pad, once the chain hangs off it.
    pad: Option<gst::Pad>,
    /// Stopped: whatever is brought up goes back down.
    gone: bool,
    /// The thread bringing it up has finished, joined or not.
    up: bool,
}

impl Joined {
    /// The encoder is stopped. Some with the pad to give back when the chain
    /// is up and the caller takes it down; None when the thread still
    /// bringing it up will.
    pub fn leave(&self) -> Option<Option<gst::Pad>> {
        let mut slot = self.0.lock();
        slot.gone = true;
        let up = slot.up;
        up.then(|| slot.pad.take())
    }

    fn gone(&self) -> bool {
        self.0.lock().gone
    }
}

/// A chain built and linked, to be brought up and joined to its bus.
pub struct Up {
    pub bus: String,
    pub ticket: u64,
    pub from: gst::Element,
    pub chain: Vec<gst::Element>,
    pub pipeline: gst::Pipeline,
    pub joined: Arc<Joined>,
    pub notify: Option<Notify>,
}

impl Up {
    /// On a thread of its own, or here when there is no thread to be had.
    pub fn start(self) {
        let bus = self.bus.clone();
        on_a_thread("bus-encoder-start", &bus, self, Up::run);
    }

    fn run(self) {
        let raised = self.raise();
        let mut slot = self.joined.0.lock();
        slot.up = true;
        if slot.gone {
            drop(slot);
            self.down().run();
            return;
        }
        let joined = raised.and_then(|_| self.join(&mut slot));
        drop(slot);
        if let Err(e) = joined {
            self.failed(format!("{e:#}"));
        }
    }

    fn raise(&self) -> Result<()> {
        for el in self.chain.iter().rev() {
            if self.joined.gone() {
                return Ok(());
            }
            el.sync_state_with_parent().with_context(|| format!("starting {}", el.name()))?;
        }
        Ok(())
    }

    /// Under the lock a stop takes, so the stop either finds the pad or has
    /// already said the encoder is gone.
    fn join(&self, slot: &mut Slot) -> Result<()> {
        let pad = self.from.request_pad_simple("src_%u").context("the bus tee refused a pad")?;
        let sink = self.chain[0].static_pad("sink").context("a queue with no sink")?;
        if let Err(e) = pad.link(&sink) {
            self.from.release_request_pad(&pad);
            return Err(anyhow::Error::from(e).context("linking a bus encoder"));
        }
        slot.pad = Some(pad);
        Ok(())
    }

    fn failed(&self, why: String) {
        tracing::warn!(bus = %self.bus, error = %why, "a bus could not be encoded; the outputs that take their sound from it come off the air and are tried again");
        if let Some(notify) = &self.notify {
            notify(AudioCall::EncoderFailed(EncoderFailed { bus: self.bus.clone(), ticket: self.ticket, why }));
        }
    }

    fn down(self) -> Down {
        Down { from: self.from, pad: None, chain: self.chain, pipeline: self.pipeline }
    }
}

/// A chain unlinked from its bus, still to be taken down.
pub struct Down {
    pub from: gst::Element,
    pub pad: Option<gst::Pad>,
    pub chain: Vec<gst::Element>,
    pub pipeline: gst::Pipeline,
}

impl Down {
    /// On a thread of its own, or here when there is no thread to be had.
    pub fn start(self, bus: &str) {
        on_a_thread("bus-encoder-stop", bus, self, Down::run);
    }

    fn run(self) {
        for el in self.chain.iter().rev() {
            el.set_locked_state(true);
            let _ = el.set_state(gst::State::Null);
        }
        if let Some(pad) = &self.pad {
            self.from.release_request_pad(pad);
        }
        let _ = self.pipeline.remove_many(&self.chain);
    }
}

/// `work(job)` on a thread from `mixer::offload`, or on this one when no
/// thread could be started.
fn on_a_thread<T: Send + 'static>(what: &'static str, id: &str, job: T, work: fn(T)) {
    let slot = Arc::new(Mutex::new(Some(job)));
    let theirs = slot.clone();
    let started = crate::mixer::offload::run(what, id, move || {
        if let Some(job) = theirs.lock().take() {
            work(job);
        }
    });
    if !started {
        if let Some(job) = slot.lock().take() {
            work(job);
        }
    }
}
