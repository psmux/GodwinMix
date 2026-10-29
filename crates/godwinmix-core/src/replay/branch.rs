//! A chain hung off a tee that ends in the ring: the GStreamer half of one
//! angle's picture or sound.
//!
//! The head of every chain is a leaky queue, so the tee above it (a source's
//! own, or the programme encoder's) only ever hands it a reference and moves
//! on. An encoder that cannot keep up drops frames in that queue and nowhere
//! else. The tail is an appsink that never waits on the clock and never
//! prerolls, whose callback does one short push into the ring.
//!
//! Joined the way `preview::feed_branch` joins: built and brought up first,
//! linked to the tee last, so a live tee never pushes into an element that
//! is still on its way up. Taken off by unlinking before anything changes
//! state.

use super::ring::Ring;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Picture or sound: which side of the ring a chain feeds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Video,
    Audio,
}

pub struct Branch {
    chain: Vec<gst::Element>,
    tee: gst::Element,
    pad: Option<gst::Pad>,
    /// Buffers into the head queue and out of it, for the drop count.
    counts: Arc<(AtomicU64, AtomicU64)>,
}

impl Branch {
    /// Add `body` to `pipeline` between a leaky queue and an appsink feeding
    /// `ring`, and join it to `tee`.
    pub fn attach(
        pipeline: &gst::Pipeline,
        tee: &gst::Element,
        queue: gst::Element,
        body: Vec<gst::Element>,
        ring: &Arc<Mutex<Ring>>,
        side: Side,
        tag: &str,
    ) -> Result<Self> {
        let counts = Arc::new((AtomicU64::new(0), AtomicU64::new(0)));
        count(&queue, "sink", &counts, true);
        count(&queue, "src", &counts, false);
        let mut chain = vec![queue];
        chain.extend(body);
        chain.push(super::sink::sink(ring, side, tag));
        pipeline.add_many(&chain).context("adding a replay angle")?;
        gst::Element::link_many(&chain).context("linking a replay angle")?;
        for el in chain.iter().rev() {
            el.sync_state_with_parent().with_context(|| format!("starting {}", el.name()))?;
        }
        let head = chain[0].static_pad("sink").context("a queue with no sink pad")?;
        let pad = tee.request_pad_simple("src_%u").context("the tee refused a pad for replay")?;
        if let Err(e) = pad.link(&head) {
            tee.release_request_pad(&pad);
            let mut half = Self { chain, tee: tee.clone(), pad: None, counts };
            half.detach();
            return Err(anyhow::Error::from(e).context("linking a replay angle to its tee"));
        }
        Ok(Self { chain, tee: tee.clone(), pad: Some(pad), counts })
    }

    /// Frames the head queue let go of because what is after it was busy.
    pub fn dropped(&self) -> u64 {
        let (into, out) = (self.counts.0.load(Ordering::Relaxed), self.counts.1.load(Ordering::Relaxed));
        let held = self.chain.first().map(|q| u64::from(q.property::<u32>("current-level-buffers"))).unwrap_or(0);
        into.saturating_sub(out).saturating_sub(held)
    }

    /// Unlink from the tee, then take every element down and out.
    pub fn detach(&mut self) {
        if let Some(pad) = self.pad.take() {
            if let Some(peer) = pad.peer() {
                let _ = pad.unlink(&peer);
            }
            self.tee.release_request_pad(&pad);
        }
        for el in &self.chain {
            el.set_locked_state(true);
            let _ = el.set_state(gst::State::Null);
            if let Some(bin) = el.parent().and_then(|p| p.downcast::<gst::Bin>().ok()) {
                let _ = bin.remove(el);
            }
        }
        self.chain.clear();
    }
}

impl Drop for Branch {
    fn drop(&mut self) {
        self.detach();
    }
}

fn count(queue: &gst::Element, pad: &str, counts: &Arc<(AtomicU64, AtomicU64)>, into: bool) {
    let Some(pad) = queue.static_pad(pad) else { return };
    let counts = counts.clone();
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        let c = if into { &counts.0 } else { &counts.1 };
        c.fetch_add(1, Ordering::Relaxed);
        gst::PadProbeReturn::Ok
    });
}
