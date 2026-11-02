//! A bus encoded for the outputs that take their sound from it.
//!
//! The same chain the programme's sound goes through, with the encoder and
//! parser the catalogue chose for this machine, so an output on a bus is fed
//! exactly what it would be fed from the programme:
//!
//! ```text
//!  bus-{id}-tee -> bus-{id}-enc-q -> audioconvert -> audiorate -> encoder -> parser -> bus-{id}-enc-tee
//! ```
//!
//! Built when the first output names the bus and taken out when the last one
//! lets go, so a bus nobody sends anywhere encodes nothing.
//!
//! Brought up on a thread of its own, then joined to the bus: on Windows the
//! encoder is Media Foundation's, and opening it on the mixer thread held the
//! `output.add` of every guest's return.

use crate::catalogue::apply::Vars;
use crate::catalogue::select::Chosen;
use crate::gstutil::{self, make};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// What every bus encoder is built from: the programme's own choice.
#[derive(Clone)]
pub struct EncodeWith {
    pub chosen: Chosen,
    pub vars: Vars,
}

pub struct BusEncoder {
    /// The encoded sound, which an output links to like `aenc-tee`.
    pub tee: gst::Element,
    /// The outputs holding it.
    pub users: Vec<String>,
    from: gst::Element,
    joined: Arc<Joined>,
    chain: Vec<gst::Element>,
    pipeline: gst::Pipeline,
}

impl BusEncoder {
    pub fn start(pipeline: &gst::Pipeline, from: &gst::Element, bus: &str, with: &EncodeWith) -> Result<Self> {
        let name = |part: &str| format!("bus-{bus}-enc-{part}");
        let enc = make(&with.chosen.element, &name("enc"))?;
        crate::catalogue::apply::apply(&enc, &with.chosen.properties, &with.vars);
        // See the programme's `aenc-rate`: a chain joined to a running bus
        // must not fill from the start of the segment.
        let rate = make("audiorate", &name("rate"))?;
        crate::probe::set_bool(&rate, "skip-to-first", true);
        let mut chain = vec![gstutil::queue_thread(&name("q"))?, make("audioconvert", &name("conv"))?, rate, enc];
        if let Some(parser) = with.chosen.parser.as_deref().filter(|p| crate::probe::exists(p)) {
            let el = make(parser, &name("parse"))?;
            crate::probe::set_int(&el, "config-interval", -1);
            chain.push(el);
        }
        let tee = make("tee", &name("tee"))?;
        tee.set_property("allow-not-linked", true);
        chain.push(tee.clone());
        pipeline.add_many(&chain).context("adding a bus encoder")?;
        gst::Element::link_many(&chain).context("linking a bus encoder")?;
        let joined = Arc::new(Joined::default());
        let up = Up { bus: bus.to_string(), from: from.clone(), chain: chain.clone(), joined: joined.clone() };
        let job = up.clone();
        if !crate::mixer::offload::run("bus-encoder-start", bus, move || job.run()) {
            up.run();
        }
        Ok(Self { tee, users: Vec::new(), from: from.clone(), joined, chain, pipeline: pipeline.clone() })
    }

    pub fn stop(self) {
        let pad = {
            let mut pad = self.joined.pad.lock();
            self.joined.gone.store(true, Ordering::SeqCst);
            pad.take()
        };
        if let Some(pad) = pad {
            if let Some(peer) = pad.peer() {
                let _ = pad.unlink(&peer);
            }
            self.from.release_request_pad(&pad);
        }
        for el in &self.chain {
            el.set_locked_state(true);
            let _ = el.set_state(gst::State::Null);
            let _ = self.pipeline.remove(el);
        }
    }
}

/// The bus tee's pad once the chain hangs off it, and whether the encoder was
/// stopped before it got there.
#[derive(Default)]
struct Joined {
    pad: Mutex<Option<gst::Pad>>,
    gone: AtomicBool,
}

/// Bringing a bus encoder up and joining it to the bus: up tail first, then
/// linked, the order `EncodeChain::attach` uses.
#[derive(Clone)]
struct Up {
    bus: String,
    from: gst::Element,
    chain: Vec<gst::Element>,
    joined: Arc<Joined>,
}

impl Up {
    fn run(&self) {
        if let Err(e) = self.raise().and_then(|_| self.join()) {
            tracing::warn!(bus = %self.bus, error = %format!("{e:#}"), "a bus could not be encoded, so the outputs that take their sound from it are silent. Remove one of them and add it again to try once more");
        }
    }

    fn raise(&self) -> Result<()> {
        for el in self.chain.iter().rev() {
            if self.joined.gone.load(Ordering::SeqCst) {
                break;
            }
            el.sync_state_with_parent().with_context(|| format!("starting {}", el.name()))?;
        }
        Ok(())
    }

    /// Under the lock `stop` takes, so a stop either finds the pad to take
    /// back or has already said the encoder is gone, and then what this
    /// brought up goes back down.
    fn join(&self) -> Result<()> {
        let mut slot = self.joined.pad.lock();
        if self.joined.gone.load(Ordering::SeqCst) {
            for el in self.chain.iter().rev() {
                let _ = el.set_state(gst::State::Null);
            }
            return Ok(());
        }
        let pad = self.from.request_pad_simple("src_%u").context("the bus tee refused a pad")?;
        let sink = self.chain[0].static_pad("sink").context("a queue with no sink")?;
        if let Err(e) = pad.link(&sink) {
            self.from.release_request_pad(&pad);
            return Err(anyhow::Error::from(e).context("linking a bus encoder"));
        }
        *slot = Some(pad);
        Ok(())
    }
}
