//! Adds nodes to a plan, sharing any node that already exists. A node's id
//! is what it does, so a second request wanting the same work finds it by id
//! and is added to its `serves` instead of getting a copy.

use std::collections::HashMap;

use godwinmix_protocol::rendition::{AudioShape, Cost, Deinterlace, EncoderPick, EncoderSlot, VideoShape};

use crate::choose::{choose, hold, Used};
use crate::container::video_slug;
use crate::error::{NoEncoder, PlanError, Skip};
use crate::graph::{Node, NodeKind, Track};
use crate::ids::{encode_id, scale_id, track_slug, Look};
use crate::model::{device_of, AudioWork, CostModel, CPU};
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
        Builder {
            model,
            encoders,
            nodes: Vec::new(),
            index: HashMap::new(),
            used: Used::new(),
        }
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
    pub fn cpu(
        &mut self,
        id: String,
        request: &str,
        inputs: Vec<String>,
        work: impl FnOnce() -> (NodeKind, Cost),
    ) -> String {
        if !self.share(&id, request) {
            let (kind, cost) = work();
            self.push(Node {
                id: id.clone(),
                kind,
                inputs,
                serves: vec![request.into()],
                device: CPU.into(),
                cost,
                reason: None,
            });
        }
        id
    }

    pub fn push(&mut self, node: Node) {
        self.index.insert(node.id.clone(), self.nodes.len());
        self.nodes.push(node);
    }

    pub fn source(&mut self, r: &Resolved) -> String {
        let source = r.source.to_string();
        self.cpu(format!("source:{source}"), r.id(), Vec::new(), || {
            (NodeKind::Source { source }, Cost::default())
        })
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
            let cost = match (track, info.video, info.audio) {
                (Track::Video, Some(v), _) => model.decode_cost(&v),
                (Track::Audio, _, Some(a)) => {
                    model.audio_cost(&a, AudioWork::Decode).unwrap_or_default()
                }
                _ => Cost::default(),
            };
            (
                NodeKind::Decode {
                    source,
                    track,
                    index: 0,
                },
                cost,
            )
        })
    }

    pub fn copied(&mut self, r: &Resolved, track: Track) -> String {
        self.copied_track(r, track, 0)
    }

    /// `copy:<source>:audio2` for sound track 2; `copy:<source>:audio` for
    /// the first, as it always was.
    pub fn copied_track(&mut self, r: &Resolved, track: Track, index: u8) -> String {
        let src = self.source(r);
        let source = r.source.to_string();
        let id = format!("copy:{source}:{}{}", track_slug(track), nth(index));
        self.cpu(id, r.id(), vec![src], || {
            (
                NodeKind::Copy {
                    source,
                    track,
                    index,
                },
                Cost::default(),
            )
        })
    }

    /// The one decoder of a further sound track, `decode:<source>:audio2`.
    pub fn decoded_track(&mut self, r: &Resolved, index: u8, shape: &AudioShape) -> String {
        if index == 0 {
            return self.decoded(r, Track::Audio);
        }
        let src = self.source(r);
        if !r.info.encoded {
            return src;
        }
        let source = r.source.to_string();
        let model = self.model;
        let shape = *shape;
        self.cpu(format!("decode:{source}:audio{index}"), r.id(), vec![src], || {
            let cost = model.audio_cost(&shape, AudioWork::Decode).unwrap_or_default();
            let track = Track::Audio;
            (NodeKind::Decode { source, track, index }, cost)
        })
    }

    /// Decode, scale when the size or rate differs, and encode, each shared.
    pub fn encoded(
        &mut self,
        r: &Resolved,
        target: &VideoShape,
        src: &VideoShape,
    ) -> Result<String, PlanError> {
        let mut upstream = self.decoded(r, Track::Video);
        let want = r.request.video.clone().unwrap_or_default();
        let look = Look {
            fit: want.fit.unwrap_or_default(),
            pad_color: want.pad_color.clone().filter(|c| !is_black(c)),
            deinterlace: want.deinterlace.unwrap_or_default(),
        };
        let resized = (target.width, target.height, target.fps) != (src.width, src.height, src.fps);
        if resized || look.deinterlace != Deinterlace::Off {
            let source = r.source.to_string();
            let (width, height, fps) = (target.width, target.height, target.fps);
            let id = scale_id(&source, target, &look);
            let model = self.model;
            upstream = self.cpu(id, r.id(), vec![upstream], || {
                (
                    NodeKind::Scale {
                        source,
                        width,
                        height,
                        fps,
                        fit: look.fit,
                        pad_color: look.pad_color,
                        deinterlace: look.deinterlace,
                    },
                    model.scale_cost(src, target),
                )
            });
        }
        let (preset, bframes) = (want.preset.clone(), want.bframes);
        let mut id = encode_id(r.source, target, preset.as_deref(), bframes);
        if let Some(pick) = want.encoder {
            id.push_str(if pick == EncoderPick::Gpu { ":gpu" } else { ":cpu" });
        }
        if self.share(&id, r.id()) {
            return Ok(id);
        }
        let picked: Vec<EncoderSlot>;
        let only = match want.encoder {
            None => &self.encoders,
            Some(p) => {
                picked = self.encoders.iter().filter(|e| e.hardware == (p == EncoderPick::Gpu)).cloned().collect();
                &picked
            }
        };
        let choice = match choose(self.model, only, target, &self.used) {
            Ok(choice) => choice,
            Err(tried) => return Err(self.no_encoder(r, target, tried)),
        };
        hold(&mut self.used, &choice.encoder, &choice.cost);
        let device = device_of(&choice.encoder).to_string();
        let kind = NodeKind::Encode {
            source: r.source.into(),
            shape: *target,
            encoder: choice.encoder,
            preset,
            bframes,
        };
        let reason = Some(choice.reason);
        let serves = vec![r.id().to_string()];
        self.push(Node {
            id: id.clone(),
            kind,
            inputs: vec![upstream],
            serves,
            device,
            cost: choice.cost,
            reason,
        });
        Ok(id)
    }

    fn no_encoder(&self, r: &Resolved, target: &VideoShape, tried: Vec<Skip>) -> PlanError {
        let found = nearest(
            self.model,
            &self.encoders,
            target,
            r.request.container,
            &self.used,
        );
        PlanError::NoEncoder(Box::new(NoEncoder {
            request: r.id().into(),
            codec: video_slug(target.codec).into(),
            shape: *target,
            tried,
            nearest: found,
        }))
    }
}

/// `""` for the first sound track, its number for the others.
fn nth(index: u8) -> String {
    match index {
        0 => String::new(),
        n => n.to_string(),
    }
}

/// Black bars are what a pad has always made, so black is no colour of its own.
fn is_black(color: &str) -> bool {
    let hex = color.trim_start_matches('#').trim_start_matches("0x");
    !hex.is_empty() && hex.chars().all(|c| c == '0')
}
