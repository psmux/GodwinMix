//! The expensive path: a filter over a composited group.
//!
//! Flattening is what makes a scene cheap. A group's transform is multiplied
//! into its children before anything reaches the compositor, so a group costs
//! no element, no pad and no copy, and nothing below the flatten step knows one
//! exists. The one thing that cannot express is a filter over the group as a
//! picture: a blur across three items at once is not three blurs.
//!
//! That needs the three items composited first, and then filtered, which means
//! a second `compositor`:
//!
//! ```text
//!   cam-a tee =|=> gate > q > crop > flip > sub pad 0 \
//!   cam-b tee =|=> gate > q > crop > flip > sub pad 1  >-- sub compositor --> caps
//!   cam-c tee =|=> gate > q > crop > flip > sub pad 2 /                        |
//!                                                                             v
//!                        programme slot:  gate > q > crop > flip > [blur] > vmix pad
//! ```
//!
//! # What it costs, and why it is only built when it is asked for
//!
//! 11 section 3 prices it: a full canvas opaque pad through a second
//! compositor is 0.14 ms a frame, which is small, and AYUV is 3.5 times I420,
//! which is not. So the sub compositor's output stays I420 unless the group
//! actually needs alpha, and the whole thing exists only while a scene has a
//! group carrying a filter. Take that filter off and the group goes back to
//! being flattened, with no element anywhere.
//!
//! The children are drawn here exactly as they would have been on the
//! programme compositor, at the same canvas coordinates, so the composite is
//! the picture the group would have made and the filter sees what an operator
//! sees.

use crate::caps::CanvasCaps;
use crate::gstutil::{self, make};
use crate::plugin::branch::ProgrammeBranch;
use crate::state::SourceId;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use tracing::{debug, warn};

/// One child of the group, on its own chain into the sub compositor.
struct Leg {
    source: SourceId,
    tee: gst::Element,
    tee_pad: gst::Pad,
    gate: gst::Element,
    crop: gst::Element,
    flip: gst::Element,
    chain: Vec<gst::Element>,
    pad: gst::Pad,
}

/// A group composited on its own, so a filter can see it as one picture.
pub struct SubCompositor {
    program: gst::Pipeline,
    canvas: CanvasCaps,
    comp: gst::Element,
    /// comp, capsfilter. The filter chain the slot carries sits below this.
    chain: Vec<gst::Element>,
    legs: Vec<Leg>,
    /// A name that is unique in the pipeline, from the slot it feeds.
    tag: String,
}

impl SubCompositor {
    /// Build one, empty. Children are added by the first `apply`.
    ///
    /// `force-live` and `ignore-inactive-pads` for the same reason the
    /// programme's compositor has them: a child that dies must not stop the
    /// group, and a group that stops is a hole in the programme.
    pub fn build(program: &gst::Pipeline, canvas: &CanvasCaps, tag: &str) -> Result<SubCompositor> {
        let comp = gstutil::make_live_aggregator("compositor", &format!("sub-comp-{tag}"))?;
        comp.set_property_from_str("background", "transparent");
        crate::probe::set_bool(&comp, "ignore-inactive-pads", true);
        // I420 out. A group that needs alpha is the 3.5 times path and is not
        // what this build offers; the reference page says so rather than the
        // picture quietly costing four times what the budget allows.
        let caps = gstutil::capsfilter(
            &format!("sub-caps-{tag}"),
            &CanvasCaps::video_at(canvas.width, canvas.height, canvas.fps),
        )?;
        let chain = vec![comp.clone(), caps];
        program.add_many(&chain).context("adding a sub compositor")?;
        gst::Element::link_many(chain.iter().collect::<Vec<_>>())
            .context("linking a sub compositor")?;
        for el in &chain {
            el.sync_state_with_parent().ok();
        }
        debug!(%tag, "a group is being composited on its own so a filter can see it whole");
        Ok(SubCompositor {
            program: program.clone(),
            canvas: canvas.clone(),
            comp,
            chain,
            legs: Vec::new(),
            tag: tag.to_string(),
        })
    }

    /// What the slot below reads.
    pub fn output(&self) -> &gst::Element {
        &self.chain[1]
    }

    /// How many children it is drawing, for the status and for a test.
    pub fn len(&self) -> usize {
        self.legs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.legs.is_empty()
    }

    /// The sources it draws, in order.
    pub fn sources(&self) -> Vec<SourceId> {
        self.legs.iter().map(|l| l.source.clone()).collect()
    }

    /// Draw this set of children.
    ///
    /// Reuses a leg already bound to the same source in the same position, so
    /// a group whose items only moved costs property writes and nothing else,
    /// which is the same promise the main pool makes.
    pub fn apply(
        &mut self,
        children: &[super::slots::Placement],
        branches: &[(&SourceId, &ProgrammeBranch)],
    ) -> Result<()> {
        let wanted: Vec<&super::slots::Placement> = children
            .iter()
            .filter(|c| branches.iter().any(|(id, _)| *id == &c.source))
            .collect();
        // Legs are positional: child 0 is leg 0. A group whose children
        // changed identity is rebuilt from that point, which is rare and
        // cheap, and keeps this far simpler than a second slot pool.
        let same = self.legs.len() == wanted.len()
            && self.legs.iter().zip(&wanted).all(|(leg, want)| leg.source == want.source);
        if !same {
            self.clear();
            for (i, child) in wanted.iter().enumerate() {
                let Some((_, branch)) = branches.iter().find(|(id, _)| *id == &child.source) else {
                    continue;
                };
                if let Err(e) = self.add_leg(i, branch) {
                    warn!(source = %child.source, ?e, "a group's child could not be composited");
                }
            }
        }
        for (leg, child) in self.legs.iter().zip(&wanted) {
            super::slots::write_pad(&leg.pad, child, &self.canvas, &leg.crop, &leg.flip);
        }
        Ok(())
    }

    /// One child: the same chain a slot has, into a pad of its own.
    fn add_leg(&mut self, index: usize, branch: &ProgrammeBranch) -> Result<()> {
        let tag = format!("{}-{index}", self.tag);
        let gate = make("valve", &format!("sub-gate-{tag}"))?;
        let queue = gstutil::queue_thread(&format!("sub-q-{tag}"))?;
        let crop = make("videocrop", &format!("sub-crop-{tag}"))?;
        let flip = make("videoflip", &format!("sub-flip-{tag}"))?;
        let chain = vec![gate.clone(), queue, crop.clone(), flip.clone()];
        self.program.add_many(&chain).context("adding a group child")?;
        gst::Element::link_many(chain.iter().collect::<Vec<_>>())
            .context("linking a group child")?;
        let pad = self
            .comp
            .request_pad_simple("sink_%u")
            .context("the sub compositor refused a pad")?;
        pad.set_property("alpha", 0.0f64);
        flip.static_pad("src")
            .context("a group child has no src pad")?
            .link(&pad)
            .context("linking a group child into its compositor")?;
        let tee_pad = branch
            .vtee
            .request_pad_simple("src_%u")
            .with_context(|| format!("the tee of {} refused a pad", branch.id))?;
        let sink = gate.static_pad("sink").context("a group child's valve has no sink pad")?;
        branch.pads.attach(&pad);
        if let Err(e) = tee_pad.link(&sink) {
            branch.pads.detach(&pad);
            branch.vtee.release_request_pad(&tee_pad);
            self.comp.release_request_pad(&pad);
            for el in &chain {
                let _ = el.set_state(gst::State::Null);
                let _ = self.program.remove(el);
            }
            return Err(e.into());
        }
        for el in &chain {
            el.sync_state_with_parent().ok();
        }
        self.legs.push(Leg {
            source: branch.id.clone(),
            tee: branch.vtee.clone(),
            tee_pad,
            gate,
            crop,
            flip,
            chain,
            pad,
        });
        Ok(())
    }

    /// Take every child off.
    fn clear(&mut self) {
        while let Some(leg) = self.legs.pop() {
            if let Some(sink) = leg.gate.static_pad("sink") {
                let _ = leg.tee_pad.unlink(&sink);
            }
            leg.tee.release_request_pad(&leg.tee_pad);
            for el in &leg.chain {
                let _ = el.set_state(gst::State::Null);
                let _ = self.program.remove(el);
            }
            self.comp.release_request_pad(&leg.pad);
        }
    }

    /// Take the whole thing out of the pipeline.
    pub fn teardown(&mut self) {
        self.clear();
        for el in &self.chain {
            let _ = el.set_state(gst::State::Null);
            let _ = self.program.remove(el);
        }
        self.chain.clear();
        debug!(tag = %self.tag, "a group's sub compositor went away");
    }
}
