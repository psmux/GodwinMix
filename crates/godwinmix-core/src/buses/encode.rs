//! A bus encoded for the outputs that take their sound from it.
//!
//! The same chain the programme's sound goes through, with the encoder and
//! parser the catalogue chose for this machine, so an output on a bus is fed
//! exactly what it would be fed from the programme:
//!
//! ```text
//!  bus-{id}-tee -> q -> audioconvert -> audiorate -> encoder -> parser -> bus-{id}-enc{n}-tee
//! ```
//!
//! Built when the first output names the bus and taken out when the last one
//! lets go, so a bus nobody sends anywhere encodes nothing. Each one is
//! numbered: the last one may still be coming down on its thread when the
//! next is built, and the two must not share a name in the pipeline.
//!
//! Brought up on a thread of its own, then joined to the bus, and taken down
//! on one too: on Windows the encoder is Media Foundation's, and opening it on
//! the mixer thread held the `output.add` of every guest's return. See `up`.

use super::Notify;
use crate::catalogue::apply::Vars;
use crate::catalogue::select::Chosen;
use crate::gstutil::{self, make};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::Arc;

mod up;
use up::{Down, Joined, Up};

/// What every bus encoder is built from: the programme's own choice.
#[derive(Clone)]
pub struct EncodeWith {
    pub chosen: Chosen,
    pub vars: Vars,
}

/// A bus encoder that would not come up, as its thread tells the mixer.
#[derive(Debug, Clone)]
pub struct EncoderFailed {
    pub bus: String,
    /// Which encoder: one taken down and built again since ignores it.
    pub ticket: u64,
    pub why: String,
}

pub struct BusEncoder {
    /// The encoded sound, which an output links to like `aenc-tee`.
    pub tee: gst::Element,
    /// The outputs holding it.
    pub users: Vec<String>,
    pub ticket: u64,
    bus: String,
    from: gst::Element,
    joined: Arc<Joined>,
    chain: Vec<gst::Element>,
    pipeline: gst::Pipeline,
}

/// Where a bus encoder is built and who hears that it failed.
pub struct Place<'a> {
    pub pipeline: &'a gst::Pipeline,
    /// The bus's own tee.
    pub from: &'a gst::Element,
    pub bus: &'a str,
    pub ticket: u64,
    pub notify: Option<Notify>,
}

impl BusEncoder {
    /// Built and linked here, brought up on a thread of its own. A failure
    /// to build answers here; a failure to come up reaches the mixer as an
    /// `AudioCall::EncoderFailed`, and with nobody to tell, the log.
    pub fn start(at: Place<'_>, with: &EncodeWith) -> Result<Self> {
        let chain = build(at.bus, at.ticket, with)?;
        let tee = chain.last().expect("a chain ends in its tee").clone();
        at.pipeline.add_many(&chain).context("adding a bus encoder")?;
        if let Err(e) = gst::Element::link_many(&chain) {
            let _ = at.pipeline.remove_many(&chain);
            return Err(anyhow::Error::from(e).context("linking a bus encoder"));
        }
        let joined = Arc::new(Joined::default());
        Up {
            bus: at.bus.to_string(),
            ticket: at.ticket,
            from: at.from.clone(),
            chain: chain.clone(),
            pipeline: at.pipeline.clone(),
            joined: joined.clone(),
            notify: at.notify,
        }
        .start();
        Ok(Self {
            tee,
            users: Vec::new(),
            ticket: at.ticket,
            bus: at.bus.to_string(),
            from: at.from.clone(),
            joined,
            chain,
            pipeline: at.pipeline.clone(),
        })
    }

    /// Unlinked from the bus here, which waits for nothing, and taken down
    /// on a thread of its own. Still coming up, it is the thread bringing it
    /// up that takes it down once it is done: taking it to NULL here would
    /// wait for the encoder to open, and taking it down beside that thread
    /// would race it element by element.
    pub fn stop(self) {
        let Some(pad) = self.joined.leave() else { return };
        if let Some((pad, peer)) = pad.as_ref().and_then(|p| p.peer().map(|peer| (p, peer))) {
            let _ = pad.unlink(&peer);
        }
        Down { from: self.from, pad, chain: self.chain, pipeline: self.pipeline }.start(&self.bus);
    }
}

/// The chain, head first, named for its bus and its number.
fn build(bus: &str, ticket: u64, with: &EncodeWith) -> Result<Vec<gst::Element>> {
    let name = |part: &str| format!("bus-{bus}-enc{ticket}-{part}");
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
    chain.push(tee);
    Ok(chain)
}
