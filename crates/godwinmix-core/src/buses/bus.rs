//! One bus: its own mixer, master and meter, in the programme pipeline.
//!
//! ```text
//!  bus-{id}-silence -> bus-{id}-silence-caps -> bus-{id}-mix:sink_0
//!  (every source's link)                     -> bus-{id}-mix:sink_%u
//!  bus-{id}-mix -> bus-{id}-caps -> bus-{id}-master -> bus-{id}-level -> bus-{id}-tee
//! ```
//!
//! The same shape as the programme's own audio: a live `audiomixer` that never
//! waits for a source that has stopped, a silence pad so it always has one
//! active input, and a tee with `allow-not-linked` that a monitor, a sound
//! card and an encoder for an output hang off. Built only when the bus is
//! asked for, and gone with it.
//!
//! A bus added an hour into a show starts its timeline at the programme's
//! running time (`start-time-selection=now`), so it does not first produce an
//! hour of silence trying to catch up.

use crate::caps::CanvasCaps;
use crate::gstutil::{self, make};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;

/// The elements of one bus.
pub struct BusGraph {
    pub id: String,
    pub mix: gst::Element,
    pub master: gst::Element,
    pub tee: gst::Element,
    /// Every element, upstream first.
    elements: Vec<gst::Element>,
    pipeline: gst::Pipeline,
}

/// The name of a bus's meter, which is how its level messages are told apart.
pub fn meter_name(id: &str) -> String {
    format!("bus-{id}-level")
}

/// The bus a meter's name belongs to.
pub fn bus_of_meter(name: &str) -> Option<&str> {
    name.strip_prefix("bus-")?.strip_suffix("-level")
}

impl BusGraph {
    /// Build the bus, bring it up from its far end, and link it. Nothing
    /// feeds it yet: the sources join through `send.rs`.
    pub fn build(pipeline: &gst::Pipeline, canvas: &CanvasCaps, id: &str, latency_ns: i64) -> Result<Self> {
        let name = |part: &str| format!("bus-{id}-{part}");
        let silence = make("audiotestsrc", &name("silence"))?;
        silence.set_property_from_str("wave", "silence");
        silence.set_property("is-live", true);
        // Stamped from the clock, so a bus started late in a show carries the
        // programme's running time from its first buffer.
        silence.set_property("do-timestamp", true);
        crate::probe::set_int(&silence, "samplesperbuffer", (canvas.sample_rate / 100) as i64);
        let silence_caps = gstutil::capsfilter(&name("silence-caps"), &canvas.audio())?;
        let mix = gstutil::make_live_aggregator("audiomixer", &name("mix"))?;
        crate::probe::set_bool(&mix, "ignore-inactive-pads", true);
        crate::probe::set_int(&mix, "min-upstream-latency", latency_ns);
        if crate::probe::try_set_enum(&mix, "start-time-selection", "now").is_err() {
            crate::probe::set_enum(&mix, "start-time-selection", "first");
        }
        let caps = gstutil::capsfilter(&name("caps"), &canvas.audio())?;
        let master = make("volume", &name("master"))?;
        let level = make("level", &meter_name(id))?;
        crate::probe::set_bool(&level, "post-messages", true);
        crate::probe::set_int(&level, "interval", 100_000_000);
        let tee = make("tee", &name("tee"))?;
        tee.set_property("allow-not-linked", true);

        let elements = vec![silence, silence_caps, mix.clone(), caps, master.clone(), level, tee.clone()];
        pipeline.add_many(&elements).context("adding a bus to the programme")?;
        let graph = Self { id: id.to_string(), mix, master, tee, elements, pipeline: pipeline.clone() };
        if let Err(e) = graph.link() {
            graph.remove();
            return Err(e);
        }
        Ok(graph)
    }

    fn link(&self) -> Result<()> {
        let [silence, silence_caps, mix, caps, master, level, tee] = &self.elements[..] else {
            anyhow::bail!("a bus has the wrong number of elements");
        };
        silence.link(silence_caps).context("linking a bus's silence")?;
        gst::Element::link_many([mix, caps, master, level, tee]).context("linking a bus")?;
        let pad = mix.request_pad_simple("sink_%u").context("the bus mixer refused its silence pad")?;
        silence_caps.static_pad("src").context("caps with no src")?.link(&pad).context("linking silence into a bus")?;
        // From the far end, so nothing is handed a buffer by an element that is
        // already running before it is ready for one.
        for el in self.elements.iter().rev() {
            el.sync_state_with_parent().with_context(|| format!("starting {}", el.name()))?;
        }
        Ok(())
    }

    pub fn set_master(&self, gain: f64, muted: bool) {
        self.master.set_property("volume", gain);
        self.master.set_property("mute", muted);
    }

    /// Take every element down and out. The links from sources and every
    /// consumer must already be gone.
    pub fn remove(&self) {
        for el in &self.elements {
            el.set_locked_state(true);
        }
        for el in &self.elements {
            let _ = el.set_state(gst::State::Null);
            let _ = self.pipeline.remove(el);
        }
        tracing::info!(bus = %self.id, "bus removed");
    }
}
