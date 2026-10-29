//! Where a source leaves for the buses: one tee in its desk, one queue and one
//! tee behind it, shared by every bus.
//!
//! ```text
//!  pgm-aq -> pgm-again -> pgm-alevel -> pgm-asend -> pgm-amute -> amix:sink_%u
//!                                           \-> pgm-abq -> pgm-abtee -> bus-a-mix:sink_%u
//!                                                                   \-> bus-b-mix:sink_%u
//! ```
//!
//! After the fader and the meter, so a bus hears what the meter shows, and
//! ahead of the mute, so the cue bus can listen to a muted microphone before
//! it goes up. The sends and the mix minus apply the mute themselves (see
//! `weight.rs`). Ahead of the source's programme side filters as well, which
//! sit after the mute; the input side ones, where sound filters go by
//! default, are upstream of all of this.
//!
//! The queue is what keeps a bus off the programme. A tee pushes to its pads
//! one after another on one thread, so without it a bus mixer that held a
//! buffer would hold the source's programme branch too. It is leaky, so a bus
//! that stops reading loses its own sound and nothing else.
//!
//! Put in while live the way `plugin::filter` puts a filter in: the pad above
//! is blocked at an idle point, the links are moved, and it is let go. Taken
//! out the same way when the last bus goes, so a programme with no bus is the
//! pipeline it always was.

use super::weight::{self, CueMarks, Programme, Shared};
use crate::gstutil::{self, make};
use crate::plugin::branch::ProgrammeBranch;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::time::Duration;

/// How long to wait for a pad to go idle. The figure `plugin::filter` uses.
const BLOCK_TIMEOUT: Duration = Duration::from_secs(5);

/// One source's way onto the buses.
pub struct SendPoint {
    pub source: String,
    /// The meter's element name, which says which build of the branch this is.
    pub meter: String,
    tee: gst::Element,
    /// The tee's pad towards the mute, which is the programme's.
    to_programme: gst::Pad,
    /// The tee's pad towards the queue.
    to_buses: gst::Pad,
    queue: gst::Element,
    fan: gst::Element,
    amute: gst::Element,
    apad: gst::Pad,
    links: Vec<Link>,
    pipeline: gst::Pipeline,
}

/// One source on one bus.
struct Link {
    bus: String,
    pad: gst::Pad,
    sink: gst::Pad,
    mix: gst::Element,
}

impl SendPoint {
    /// Put the tee into a running branch.
    pub fn insert(pipeline: &gst::Pipeline, branch: &ProgrammeBranch) -> Result<Self> {
        let tag = branch.meter.strip_prefix("pgm-alevel-").unwrap_or(&branch.meter).to_string();
        let tee = make("tee", &format!("pgm-asend-{tag}"))?;
        tee.set_property("allow-not-linked", true);
        let queue = gstutil::queue_preview_holding(&format!("pgm-abq-{tag}"), 0.5)?;
        let fan = make("tee", &format!("pgm-abtee-{tag}"))?;
        fan.set_property("allow-not-linked", true);
        pipeline.add_many([&tee, &queue, &fan]).context("adding a source's send point")?;
        let built = Self::bring_up(&tee, &queue, &fan);
        let (to_programme, to_buses) = match built {
            Ok(pads) => pads,
            Err(e) => {
                drop_elements(pipeline, &[&tee, &queue, &fan]);
                return Err(e);
            }
        };
        let point = Self {
            source: branch.id.clone(),
            meter: branch.meter.clone(),
            tee,
            to_programme,
            to_buses,
            queue,
            fan,
            amute: branch.amute.clone(),
            apad: branch.apad.clone(),
            links: Vec::new(),
            pipeline: pipeline.clone(),
        };
        if let Err(e) = point.splice_in() {
            point.drop_own();
            return Err(e);
        }
        tracing::debug!(source = %point.source, "send point in");
        Ok(point)
    }

    /// Everything up and linked behind the tee, before the tee sees a buffer.
    fn bring_up(tee: &gst::Element, queue: &gst::Element, fan: &gst::Element) -> Result<(gst::Pad, gst::Pad)> {
        queue.link(fan).context("linking a send queue")?;
        for el in [fan, queue, tee] {
            el.sync_state_with_parent().with_context(|| format!("starting {}", el.name()))?;
        }
        let to_programme = tee.request_pad_simple("src_%u").context("the send tee refused a pad")?;
        let to_buses = tee.request_pad_simple("src_%u").context("the send tee refused a pad")?;
        to_buses
            .link(&queue.static_pad("sink").context("a queue with no sink")?)
            .context("linking the send tee to its queue")?;
        Ok((to_programme, to_buses))
    }

    /// Move the link above the mute onto the tee, at an idle point.
    fn splice_in(&self) -> Result<()> {
        let mute_in = self.amute.static_pad("sink").context("the mute has no sink pad")?;
        let above = mute_in.peer().context("nothing is linked above the mute")?;
        let tee_in = self.tee.static_pad("sink").context("a tee with no sink")?;
        let (a, to_p) = (above.clone(), self.to_programme.clone());
        gstutil::with_pad_blocked(&above, BLOCK_TIMEOUT, move || {
            let _ = a.unlink(&mute_in);
            let linked = a.link(&tee_in).map_err(|e| e.to_string()).and_then(|_| to_p.link(&mute_in).map_err(|e| e.to_string()));
            if let Err(e) = linked {
                tracing::error!(%e, "could not put a send point in; putting the mute back");
                let _ = a.unlink(&tee_in);
                let _ = to_p.unlink(&mute_in);
                let _ = a.link(&mute_in);
            }
        })
        .context("putting a send point in while blocked")?;
        anyhow::ensure!(self.to_programme.is_linked(), "the send point did not go in; the source is as it was");
        Ok(())
    }

    /// Put this source on `bus`.
    pub fn join(&mut self, bus: &str, mix: &gst::Element, shared: Shared, cue: CueMarks) -> Result<()> {
        if self.links.iter().any(|l| l.bus == bus) {
            return Ok(());
        }
        let sink = mix.request_pad_simple("sink_%u").context("the bus mixer refused a pad")?;
        sink.set_property("volume", 0.0f64);
        sink.set_offset(self.apad.offset());
        let pad = self.fan.request_pad_simple("src_%u").context("the send fan refused a pad")?;
        let programme = Programme { source: self.source.clone(), amute: self.amute.clone(), apad: self.apad.clone() };
        weight::follow(&pad, &sink, shared, cue, programme);
        if let Err(e) = pad.link(&sink) {
            self.fan.release_request_pad(&pad);
            mix.release_request_pad(&sink);
            return Err(anyhow::Error::from(e).context(format!("linking {} to bus {bus}", self.source)));
        }
        self.links.push(Link { bus: bus.to_string(), pad, sink, mix: mix.clone() });
        Ok(())
    }

    /// Take this source off `bus`. Unlinked at an idle point before either pad
    /// is released: a mixer pad released under a buffer answers FLUSHING, and
    /// that must never reach the queue, whose upstream is the programme.
    pub fn leave(&mut self, bus: &str) {
        let Some(pos) = self.links.iter().position(|l| l.bus == bus) else { return };
        let link = self.links.remove(pos);
        let (pad, sink) = (link.pad.clone(), link.sink.clone());
        if let Err(e) = gstutil::with_pad_blocked(&link.pad, BLOCK_TIMEOUT, move || {
            let _ = pad.unlink(&sink);
        }) {
            tracing::warn!(source = %self.source, bus, ?e, "a bus link would not go idle; unlinking it anyway");
            let _ = link.pad.unlink(&link.sink);
        }
        self.fan.release_request_pad(&link.pad);
        link.mix.release_request_pad(&link.sink);
    }

    pub fn buses(&self) -> impl Iterator<Item = &str> {
        self.links.iter().map(|l| l.bus.as_str())
    }

    /// Take the tee out of the branch and everything behind it out of the
    /// pipeline, leaving the branch as it was before the first bus.
    pub fn remove(mut self) {
        let buses: Vec<String> = self.buses().map(str::to_string).collect();
        for bus in buses {
            self.leave(&bus);
        }
        if let Err(e) = self.splice_out() {
            tracing::error!(source = %self.source, ?e, "could not take a send point out");
        }
        self.drop_own();
        tracing::debug!(source = %self.source, "send point out");
    }

    fn splice_out(&self) -> Result<()> {
        let tee_in = self.tee.static_pad("sink").context("a tee with no sink")?;
        let mute_in = self.amute.static_pad("sink").context("the mute has no sink pad")?;
        let Some(above) = tee_in.peer() else { return Ok(()) };
        let (a, to_p) = (above.clone(), self.to_programme.clone());
        gstutil::with_pad_blocked(&above, BLOCK_TIMEOUT, move || {
            let _ = a.unlink(&tee_in);
            let _ = to_p.unlink(&mute_in);
            if let Err(e) = a.link(&mute_in) {
                tracing::error!(?e, "could not relink a source's mute after its send point");
            }
        })
        .context("taking a send point out while blocked")
    }

    /// The tee, queue and fan, unlinked from everything, down and out.
    fn drop_own(&self) {
        if let Some(q) = self.queue.static_pad("sink") {
            let _ = self.to_buses.unlink(&q);
        }
        self.tee.release_request_pad(&self.to_buses);
        self.tee.release_request_pad(&self.to_programme);
        drop_elements(&self.pipeline, &[&self.tee, &self.queue, &self.fan]);
    }
}

fn drop_elements(pipeline: &gst::Pipeline, els: &[&gst::Element]) {
    for el in els {
        el.set_locked_state(true);
        let _ = el.set_state(gst::State::Null);
        let _ = pipeline.remove(*el);
    }
}
