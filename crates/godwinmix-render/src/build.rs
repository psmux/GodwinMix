//! Adds nodes to a plan, sharing any node that already exists. A node's id
//! is what it does, so a second request wanting the same work finds it by id
//! and is added to its `serves` instead of getting a copy.

use std::collections::HashMap;

use godwinmix_protocol::rendition::{Cost, EncoderSlot, VideoShape};

use crate::choose::{choose, hold, Used};
use crate::container::video_slug;
use crate::error::{NoEncoder, PlanError, Skip};
use crate::graph::{Node, NodeKind, Track};
use crate::ids::{encode_id, scale_id, track_slug};
use crate::model::{device_of, CostModel, CPU};
use crate::nearest::nearest;
use crate::plan::Resolved;

pub struct Builder<'m> {
    pub model: &'m dyn CostModel,
    pub encoders: Vec<EncoderSlot>,
    pub nodes: Vec<Node>,
    index: HashMap<String, usize>,
    used: Used,
}

impl<'m> Builder<'m> {
    pub fn new(model: &'m dyn CostModel, encoders: Vec<EncoderSlot>) -> Self {
        Builder { model, encoders, nodes: Vec::new(), index: HashMap::new(), used: Used::new() }
    }

    /// Adds `request` to an existing node's `serves`. False when there is no
    /// such node yet.
    pub fn share(&mut self, id: &str, request: &str) -> bool {
        let Some(&i) = self.index.get(id) else {
            return false;
        };
        let serves = &mut self.nodes[i].serves;
        if !serves.iter().any(|r| r == request) {
            serves.push(request.to_string());
        }
        true
    }

    /// Adds a node doing `kind` for `request` on the CPU, unless one exists.
    pub fn cpu(&mut self, id: String, request: &str, inputs: Vec<String>, work: impl FnOnce() -> (NodeKind, Cost)) -> String {
        if !self.share(&id, request) {
            let (kind, cost) = work();
            self.push(Node { id: id.clone(), kind, inputs, serves: vec![request.into()], device: CPU.into(), cost, reason: None });
        }
        id
    }

    pub fn push(&mut self, node: Node) {
        self.index.insert(node.id.clone(), self.nodes.len());
        self.nodes.push(node);
    }

    pub fn source(&mut self, r: &Resolved) -> String {
        let source = r.source.to_string();
        self.cpu(format!("source:{source}"), r.id(), Vec::new(), || (NodeKind::Source { source }, Cost::default()))
    }

    /// The source's one decoder for a track, or the source itself when its
    /// frames are raw already.
    pub fn decoded(&mut self, r: &Resolved, track: Track) -> String {
        let src = self.source(r);
        if !r.info.encoded {
            return src;
        }
        let tag = track_slug(track);
        let source = r.source.to_string();
        let model = self.model;
        let info = r.info;
        self.cpu(format!("decode:{source}:{tag}"), r.id(), vec![src], || {
            let cost = match (track, info.video) {
                (Track::Video, Some(v)) => model.decode_cost(&v),
                _ => Cost::default(),
            };
            (NodeKind::Decode { source, track }, cost)
        })
    }

    pub fn copied(&mut self, r: &Resolved, track: Track) -> String {
        let src = self.source(r);
        let source = r.source.to_string();
        let id = format!("copy:{source}:{}", track_slug(track));
        self.cpu(id, r.id(), vec![src], || (NodeKind::Copy { source, track }, Cost::default()))
    }

    /// Decode, scale when the size or rate differs, and encode, each shared.
    pub fn encoded(&mut self, r: &Resolved, target: &VideoShape, src: &VideoShape) -> Result<String, PlanError> {
        let mut upstream = self.decoded(r, Track::Video);
        if (target.width, target.height, target.fps) != (src.width, src.height, src.fps) {
            let source = r.source.to_string();
            let (width, height, fps) = (target.width, target.height, target.fps);
            let id = scale_id(&source, target);
            let model = self.model;
            upstream = self.cpu(id, r.id(), vec![upstream], || {
                (NodeKind::Scale { source, width, height, fps }, model.scale_cost(src, target))
            });
        }
        let id = encode_id(r.source, target);
        if self.share(&id, r.id()) {
            return Ok(id);
        }
        let choice = match choose(self.model, &self.encoders, target, &self.used) {
            Ok(choice) => choice,
            Err(tried) => return Err(self.no_encoder(r, target, tried)),
        };
        hold(&mut self.used, &choice.encoder, &choice.cost);
        let device = device_of(&choice.encoder).to_string();
        let kind = NodeKind::Encode { source: r.source.into(), shape: *target, encoder: choice.encoder };
        let reason = Some(choice.reason);
        let serves = vec![r.id().to_string()];
        self.push(Node { id: id.clone(), kind, inputs: vec![upstream], serves, device, cost: choice.cost, reason });
        Ok(id)
    }

    fn no_encoder(&self, r: &Resolved, target: &VideoShape, tried: Vec<Skip>) -> PlanError {
        let found = nearest(self.model, &self.encoders, target, r.request.container, &self.used);
        PlanError::NoEncoder(Box::new(NoEncoder {
            request: r.id().into(),
            codec: video_slug(target.codec).into(),
            shape: *target,
            tried,
            nearest: found,
        }))
    }
}
