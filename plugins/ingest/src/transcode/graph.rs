//! One stream's running pipeline, changed a node at a time.
//!
//! The core's plan names every node by the work it does, so a node whose
//! description is the same as before is left running, untouched, and only
//! the ones that changed (and whatever reads from them) are taken down and
//! built again. A branch is taken off its tee when the tee's pad is idle, so
//! nothing is unlinked in the middle of a buffer, and the rest of the
//! pipeline keeps playing throughout.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use gstreamer as gst;
use gstreamer::prelude::*;

use super::build::{build, Built};
use super::input::Input;
use super::sink::{attach, tap_pictures, Route};
use super::spec::NodeSpec;
use crate::media_tag::TagKind;

struct Running {
    spec: NodeSpec,
    built: Built,
    /// The upstream tee's pad this node hangs off.
    feed: Option<gst::Pad>,
}

pub struct Graph {
    pipeline: gst::Pipeline,
    running: Vec<Running>,
    /// Nodes that would not start, and why.
    pub failed: HashMap<String, String>,
}

impl Graph {
    pub fn new() -> Graph {
        let pipeline = gst::Pipeline::with_name("gmx-transcode");
        let _ = pipeline.set_state(gst::State::Playing);
        Graph { pipeline, running: Vec::new(), failed: HashMap::new() }
    }

    /// Make what runs match `specs`: stop what changed or went, and
    /// everything reading from it, then start what is new.
    pub fn apply(&mut self, specs: &[NodeSpec], input: &Input, route: &Arc<dyn Route>) {
        let mut stop: Vec<String> = self.running.iter().filter(|r| !specs.contains(&r.spec)).map(|r| r.spec.id.clone()).collect();
        // Readers of a stopped node stop with it, however far downstream.
        loop {
            let more: Vec<String> = self
                .running
                .iter()
                .filter(|r| !stop.contains(&r.spec.id) && stop.contains(&r.spec.input))
                .map(|r| r.spec.id.clone())
                .collect();
            if more.is_empty() {
                break;
            }
            stop.extend(more);
        }
        for id in self.running.iter().rev().map(|r| r.spec.id.clone()).filter(|id| stop.contains(id)).collect::<Vec<_>>() {
            self.stop(&id, input);
        }
        self.failed.retain(|id, _| specs.iter().any(|s| &s.id == id));
        for spec in specs {
            if self.running.iter().any(|r| r.spec.id == spec.id) {
                continue;
            }
            if let Err(why) = self.start(spec, input, route) {
                eprintln!("transcode: node {} did not start: {why}", spec.id);
                self.failed.insert(spec.id.clone(), why);
            }
        }
    }

    fn start(&mut self, spec: &NodeSpec, input: &Input, route: &Arc<dyn Route>) -> Result<(), String> {
        let upstream = match spec.input.as_str() {
            "" => None,
            id => Some(self.running.iter().find(|r| r.spec.id == id).and_then(|r| r.built.tee.clone()).ok_or_else(|| {
                self.failed.get(id).cloned().unwrap_or_else(|| format!("node {id}, which {} reads, is not running", spec.id))
            })?),
        };
        let built = build(spec)?;
        let els: Vec<&gst::Element> = built.elements.iter().collect();
        self.pipeline.add_many(els.iter().copied()).map_err(|e| e.to_string())?;
        if let Err(e) = gst::Element::link_many(els.iter().copied()) {
            self.discard(&built.elements);
            return Err(format!("the elements of {} would not link: {e}", spec.id));
        }
        if let Some(sink) = &built.appsink {
            let kind = if spec.kind == "aencode" { TagKind::Audio } else { TagKind::Video };
            attach(sink, kind, spec.id.clone(), route.clone());
        }
        for el in built.elements.iter().rev() {
            let _ = el.sync_state_with_parent();
        }
        let feed = match (upstream, &built.head) {
            (Some(tee), Some(head)) => match hang(&tee, head) {
                Ok(pad) => Some(pad),
                Err(why) => {
                    self.discard(&built.elements);
                    return Err(format!("{} would not link to its input: {why}", spec.id));
                }
            },
            _ => None,
        };
        if let ("decode", "video", Some(tee)) = (spec.kind.as_str(), spec.track(), &built.tee) {
            tap_pictures(tee, spec.id.clone(), route.clone());
        }
        if let Some(src) = &built.appsrc {
            let track = if spec.track() == "audio" { TagKind::Audio } else { TagKind::Video };
            input.attach(track, Some(src.clone()));
        }
        self.running.push(Running { spec: spec.clone(), built, feed });
        Ok(())
    }

    fn stop(&mut self, id: &str, input: &Input) {
        let Some(at) = self.running.iter().position(|r| r.spec.id == id) else { return };
        let r = self.running.remove(at);
        if r.built.appsrc.is_some() {
            input.attach(if r.spec.track() == "audio" { TagKind::Audio } else { TagKind::Video }, None);
        }
        if let (Some(pad), Some(head)) = (&r.feed, &r.built.head) {
            unlink_when_idle(pad, head);
            if let Some(tee) = pad.parent_element() {
                tee.release_request_pad(pad);
            }
        }
        self.discard(&r.built.elements);
    }

    fn discard(&self, elements: &[gst::Element]) {
        for el in elements.iter() {
            let _ = el.set_state(gst::State::Null);
            let _ = self.pipeline.remove(el);
        }
    }

    /// Errors the pipeline raised since the last look, by the node whose
    /// element raised them.
    pub fn errors(&mut self) -> Vec<(String, String)> {
        let Some(bus) = self.pipeline.bus() else { return Vec::new() };
        let mut out = Vec::new();
        while let Some(msg) = bus.pop_filtered(&[gst::MessageType::Error]) {
            let gst::MessageView::Error(e) = msg.view() else { continue };
            let from = msg.src().map(|s| s.name().to_string()).unwrap_or_default();
            let node = self.running.iter().find(|r| r.built.elements.iter().any(|el| el.name() == from));
            if let Some(r) = node {
                out.push((r.spec.id.clone(), format!("{from}: {}", e.error())));
            }
        }
        for (id, why) in &out {
            self.failed.insert(id.clone(), why.clone());
        }
        out
    }
}

impl Drop for Graph {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

/// Hang a branch off a tee: a new pad of the tee, linked to the branch's head.
fn hang(tee: &gst::Element, head: &gst::Element) -> Result<gst::Pad, String> {
    let pad = tee.request_pad_simple("src_%u").ok_or("the tee gave no pad")?;
    let sink = head.static_pad("sink").ok_or("the branch has no sink pad")?;
    if let Err(e) = pad.link(&sink) {
        tee.release_request_pad(&pad);
        return Err(format!("{e:?}"));
    }
    Ok(pad)
}

/// Unlink a branch from its tee once no buffer is on the way through, and
/// wait a moment for that; a tee with nothing flowing is idle at once.
fn unlink_when_idle(pad: &gst::Pad, head: &gst::Element) {
    let Some(sink) = head.static_pad("sink") else { return };
    let (tx, rx) = std::sync::mpsc::sync_channel::<()>(1);
    pad.add_probe(gst::PadProbeType::IDLE, move |pad, _| {
        let _ = pad.unlink(&sink);
        let _ = tx.try_send(());
        gst::PadProbeReturn::Remove
    });
    let _ = rx.recv_timeout(Duration::from_millis(500));
}
