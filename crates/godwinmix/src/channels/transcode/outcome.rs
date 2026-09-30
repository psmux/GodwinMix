//! What one destination got from the plan, and how the listener is told.

use godwinmix_protocol::destination::{DestinationMode, DestinationPlan, DestinationRefusal};
use godwinmix_protocol::rendition::StreamInfo;
use godwinmix_render::{NodeKind, Plan, ReasonCode, Track};

/// One destination's lot.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Its stream is not live, or its codecs are not known yet.
    Waiting,
    /// Every track the stream has goes out as it arrived: the listener
    /// sends it down the copy path, exactly as a destination that asked for
    /// nothing.
    Copy(DestinationPlan),
    /// Read from the plan's nodes. `video` and `audio` name the node each
    /// track comes from: an encode, or `copy:<stream>:<track>` for the
    /// stream's own.
    Transcode { plan: DestinationPlan, video: Option<String>, audio: Option<String> },
    Refused(DestinationRefusal),
}

impl Outcome {
    pub fn plan(&self) -> Option<&DestinationPlan> {
        match self {
            Outcome::Copy(p) | Outcome::Transcode { plan: p, .. } => Some(p),
            _ => None,
        }
    }
}

/// The outcome of a request the plan serves.
pub fn describe(plan: &Plan, id: &str, stream: &str, info: &StreamInfo) -> Outcome {
    let Some(mux) = plan.output(id) else { return Outcome::Waiting };
    let (mut video, mut audio) = (None, None);
    for input in &mux.inputs {
        match plan.node(input).map(|n| &n.kind) {
            Some(NodeKind::Copy { track: Track::Video, .. } | NodeKind::Encode { .. }) => video = Some(input.clone()),
            Some(NodeKind::Copy { track: Track::Audio, .. } | NodeKind::AudioEncode { .. }) => audio = Some(input.clone()),
            _ => {}
        }
    }
    let copied = |node: &Option<String>, has: bool| match node {
        Some(n) => n.starts_with("copy:"),
        None => !has,
    };
    let all_copy = copied(&video, info.video.is_some()) && copied(&audio, info.audio.is_some());
    let mut chain: Vec<String> = plan
        .chain(id)
        .iter()
        .filter(|n| !matches!(n.kind, NodeKind::Source { .. } | NodeKind::Mux { .. }))
        .map(|n| n.id.clone())
        .collect();
    chain.reverse();
    let encode = plan.chain(id).into_iter().find(|n| matches!(n.kind, NodeKind::Encode { .. }));
    let aencode = plan.chain(id).into_iter().find_map(|n| match &n.kind {
        NodeKind::AudioEncode { shape, .. } => Some(*shape),
        _ => None,
    });
    let reason = mux.reason.as_ref().map(|r| r.text.clone()).unwrap_or_default();
    let view = DestinationPlan {
        mode: if all_copy { DestinationMode::Copy } else { DestinationMode::Transcode },
        stream: stream.to_string(),
        reason: if all_copy && mux.reason.as_ref().is_some_and(|r| r.code != ReasonCode::Copied) {
            "copied: the stream goes out as it arrives".into()
        } else {
            reason
        },
        encoder: encode.and_then(|n| match &n.kind {
            NodeKind::Encode { encoder, .. } => Some(encoder.id.clone()),
            _ => None,
        }),
        encoder_reason: encode.and_then(|n| n.reason.as_ref().map(|r| r.text.clone())),
        video: match encode.map(|n| &n.kind) {
            Some(NodeKind::Encode { shape, .. }) => Some(*shape),
            _ => video.as_ref().and(info.video),
        },
        audio: aencode.or(audio.as_ref().and(info.audio)),
        nodes: chain,
    };
    if all_copy {
        return Outcome::Copy(view);
    }
    Outcome::Transcode { plan: view, video, audio }
}
