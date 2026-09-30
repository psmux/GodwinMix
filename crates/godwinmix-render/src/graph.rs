//! The plan: a graph of nodes with stable ids, in the order they must start.

use std::collections::BTreeMap;

use godwinmix_protocol::rendition::{AudioShape, Container, Cost, EncoderSlot, Fps, VideoShape};
use serde::Serialize;

/// Video or audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Track {
    Video,
    Audio,
}

/// What one node does. Two nodes with the same id and the same kind are the
/// same running work; `diff` restarts a node only when its kind or its
/// inputs changed.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum NodeKind {
    /// Where a source's frames or bytes come from: the hub or the frame bus.
    Source { source: String },
    /// The source's encoded bytes, parsed and passed on untouched.
    Copy { source: String, track: Track },
    /// The source's one decoder for this track.
    Decode { source: String, track: Track },
    /// Scale, convert and change the frame rate, once per distinct result.
    Scale { source: String, width: u32, height: u32, fps: Fps },
    /// One video encoder, shared by every output that wants this shape.
    Encode { source: String, shape: VideoShape, encoder: EncoderSlot },
    /// Resample or remix, once per distinct result.
    AudioConvert { source: String, channels: u8, sample_rate: u32 },
    /// One audio encoder, shared by every output that wants this shape.
    AudioEncode { source: String, shape: AudioShape },
    /// One output's container. Every request has exactly one.
    Mux { request: String, container: Container },
}

/// Why the planner decided what it did, for the page to show as it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Reason {
    pub code: ReasonCode,
    /// One sentence a person reads: "using h264-software-x264 because the
    /// GPU nvidia0 is full".
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReasonCode {
    /// A hardware encoder of the right codec had room.
    Hardware,
    /// This machine has no hardware encoder for the codec.
    SoftwareOnly,
    /// A better encoder's device is full, so the next one was taken.
    DeviceFull,
    /// A better encoder cannot make this shape, so the next one was taken.
    ShapeUnsupported,
    /// The output takes the source's own bytes.
    Copied,
    /// The output needs an encode; the text says what differs.
    Transcoded,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Node {
    /// Stable across plans: built from what the node does, never a counter.
    pub id: String,
    #[serde(flatten)]
    pub kind: NodeKind,
    /// Ids of the nodes this one reads from.
    pub inputs: Vec<String>,
    /// Ids of the requests this node works for.
    pub serves: Vec<String>,
    /// Which device the cost is counted against: `cpu`, or a GPU's name.
    pub device: String,
    pub cost: Cost,
    /// Set on Encode nodes (which encoder and why) and on Mux nodes (copied
    /// or transcoded, and why).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<Reason>,
}

/// The smallest graph that serves every request.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Plan {
    /// Producers before consumers, so starting them in this order works.
    pub nodes: Vec<Node>,
    /// The one keyframe interval every encode of a source uses.
    pub keyframe_ms: BTreeMap<String, u32>,
    /// What the plan costs on each device.
    pub cost: BTreeMap<String, Cost>,
    pub total: Cost,
}

impl Plan {
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// The Mux node that serves one request.
    pub fn output(&self, request: &str) -> Option<&Node> {
        self.node(&format!("mux:{request}"))
    }

    pub fn encodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.iter().filter(|n| matches!(n.kind, NodeKind::Encode { .. }))
    }

    pub fn count(&self, pred: impl Fn(&NodeKind) -> bool) -> usize {
        self.nodes.iter().filter(|n| pred(&n.kind)).count()
    }

    /// The nodes that feed a request's output, walking back from its Mux.
    pub fn chain(&self, request: &str) -> Vec<&Node> {
        let mut out: Vec<&Node> = Vec::new();
        let mut todo = vec![format!("mux:{request}")];
        while let Some(id) = todo.pop() {
            if let Some(node) = self.node(&id) {
                if out.iter().any(|n| n.id == node.id) {
                    continue;
                }
                todo.extend(node.inputs.iter().cloned());
                out.push(node);
            }
        }
        out
    }
}
