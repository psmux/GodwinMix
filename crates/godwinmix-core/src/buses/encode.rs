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

use crate::catalogue::apply::Vars;
use crate::catalogue::select::Chosen;
use crate::gstutil::{self, make};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;

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
    pad: gst::Pad,
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
        for el in chain.iter().rev() {
            el.sync_state_with_parent().with_context(|| format!("starting {}", el.name()))?;
        }
        let pad = from.request_pad_simple("src_%u").context("the bus tee refused a pad")?;
        pad.link(&chain[0].static_pad("sink").context("a queue with no sink")?).context("linking a bus encoder")?;
        Ok(Self { tee, users: Vec::new(), from: from.clone(), pad, chain, pipeline: pipeline.clone() })
    }

    pub fn stop(self) {
        if let Some(peer) = self.pad.peer() {
            let _ = self.pad.unlink(&peer);
        }
        self.from.release_request_pad(&self.pad);
        for el in &self.chain {
            el.set_locked_state(true);
            let _ = el.set_state(gst::State::Null);
            let _ = self.pipeline.remove(el);
        }
    }
}
