//! A plan, running. Every node has an outlet (a tee its consumers hang off)
//! that lives as long as the node's id is in the plan, and a body (queue,
//! work, and the encoder where there is one) that can be stopped and started
//! under it: by a replan that restarts the node, or by the governor shedding
//! it. Consumers stay linked to the outlet through both.
//!
//! Everything here runs on the mixer thread, which owns the programme
//! pipeline's state changes; never on a streaming thread or the bus handler.

use super::elements;
use super::wiring::{self, is_audio, take_down, Live};
use crate::catalogue::select::GraphicsChoice;
use crate::catalogue::Catalogue;
use anyhow::{Context, Result};
use godwinmix_render::{Node, NodeKind, Plan};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::collections::HashMap;
use tracing::debug;

/// One running node.
struct Running {
    outlet: gst::Element,
    /// False for the programme's own tee, which this does not own.
    owned: bool,
    body: Option<Live>,
}

/// Where the programme's raw frames and samples are.
pub struct Programme {
    pub pipeline: gst::Pipeline,
    pub video: gst::Element,
    pub audio: gst::Element,
    pub gfx: GraphicsChoice,
    /// The video offset the programme encoder carries, so a rendition's
    /// picture and sound line up exactly as the programme's do.
    pub av_offset_ns: i64,
}

pub struct Graph {
    programme: Programme,
    nodes: HashMap<String, Running>,
}

impl Graph {
    pub fn new(programme: Programme) -> Self {
        Graph { programme, nodes: HashMap::new() }
    }

    pub fn pipeline(&self) -> &gst::Pipeline {
        &self.programme.pipeline
    }

    /// The tee a node's consumers read, while it is in the plan.
    pub fn outlet(&self, id: &str) -> Option<gst::Element> {
        self.nodes.get(id).map(|n| n.outlet.clone())
    }

    /// Bring a node into the running graph: its outlet, then its body.
    pub fn start(&mut self, node: &Node, plan: &Plan, cat: &Catalogue, preset: Option<&str>) -> Result<()> {
        if self.nodes.contains_key(&node.id) {
            return self.resume(node, plan, cat, preset);
        }
        let name = format!("r-{}", elements::slug(&node.id));
        if let NodeKind::Source { .. } = node.kind {
            if !self.programme.gfx.is_gpu() {
                // The raw programme tee is already a tee in system memory.
                let outlet = self.programme.video.clone();
                self.nodes.insert(node.id.clone(), Running { outlet, owned: false, body: None });
                return Ok(());
            }
        }
        let outlet = wiring::outlet(&name)?;
        self.programme.pipeline.add(&outlet).context("adding a rendition outlet")?;
        outlet.sync_state_with_parent().ok();
        self.nodes.insert(node.id.clone(), Running { outlet, owned: true, body: None });
        self.resume(node, plan, cat, preset)
    }

    /// Start a node's body under its existing outlet. Nothing when it runs.
    pub fn resume(&mut self, node: &Node, plan: &Plan, cat: &Catalogue, preset: Option<&str>) -> Result<()> {
        let running = self.nodes.get(&node.id).context("resuming a node that was never started")?;
        if running.body.is_some() || !running.owned {
            return Ok(());
        }
        let outlet = running.outlet.clone();
        let name = format!("r-{}", elements::slug(&node.id));
        let Some(body) = elements::body(&node.kind, &name, cat, &self.programme.gfx, preset)? else {
            return Ok(());
        };
        let upstream = self.upstream(node)?;
        let live = wiring::link(&self.programme.pipeline, self.programme.av_offset_ns, body, &upstream, &outlet, node, plan)?;
        if let Some(r) = self.nodes.get_mut(&node.id) {
            r.body = Some(live);
        }
        debug!(node = %node.id, "rendition node started");
        Ok(())
    }

    /// The tee a node's body reads from.
    fn upstream(&self, node: &Node) -> Result<gst::Element> {
        if let NodeKind::Source { .. } = node.kind {
            return Ok(self.programme.video.clone());
        }
        let input = node.inputs.first().context("a node with nothing upstream")?;
        if input.starts_with("source:") && is_audio(&node.kind) {
            return Ok(self.programme.audio.clone());
        }
        self.outlet(input).with_context(|| format!("{} reads {input}, which is not running", node.id))
    }

    /// Take a node's body down, leaving its outlet and its consumers.
    pub fn suspend(&mut self, id: &str) {
        let Some(live) = self.nodes.get_mut(id).and_then(|r| r.body.take()) else { return };
        let pipeline = self.programme.pipeline.clone();
        take_down(&pipeline, live);
        debug!(node = %id, "rendition node stopped");
    }

    /// Take a node out altogether: body and outlet.
    pub fn stop(&mut self, id: &str) {
        self.suspend(id);
        let Some(r) = self.nodes.remove(id) else { return };
        if r.owned {
            wiring::retire(&self.programme.pipeline, vec![r.outlet], None);
        }
    }

    /// Everything, for shutdown and for a test's clean up.
    pub fn stop_all(&mut self) {
        let ids: Vec<String> = self.nodes.keys().cloned().collect();
        for id in ids {
            self.stop(&id);
        }
    }

}
