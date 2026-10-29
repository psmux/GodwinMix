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

use crate::gstutil::{self, make};
use crate::plugin::branch::ProgrammeBranch;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::time::Duration;

mod links;
mod splice;
use splice::drop_elements;

/// How long to wait for a pad to go idle. The figure `plugin::filter` uses.
pub(super) const BLOCK_TIMEOUT: Duration = Duration::from_secs(5);

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

}
