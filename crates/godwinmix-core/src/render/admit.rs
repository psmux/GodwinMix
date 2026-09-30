//! Asking the governor before a node starts. An encode asks with its shape,
//! so the governor can price it from calibration and pick a software preset
//! that fits; a scale or an audio encode asks with what the plan priced it
//! at. Nothing that costs nothing asks.

use super::refusal::Refusal;
use godwinmix_govern::{Admit, Claim, Governor, Kind, Ticket};
use godwinmix_protocol::rendition::RenditionRequest;
use godwinmix_render::{container::video_name, Node, NodeKind};
use std::collections::HashMap;

/// A ladder rung's position: 0 is the top, never shed.
pub type Rungs = HashMap<String, u8>;

/// Words for a node, for a refusal and a shed alert: "the 720p30 H.264
/// rendition for yt, fb".
pub fn describe(node: &Node) -> String {
    let who = node.serves.join(", ");
    match &node.kind {
        NodeKind::Encode { shape, .. } => format!(
            "{}p{} {} rendition for {who}",
            shape.height,
            shape.fps.num / shape.fps.den.max(1),
            video_name(shape.codec)
        ),
        NodeKind::Scale { width, height, .. } => format!("{width}x{height} scaling for {who}"),
        NodeKind::AudioEncode { .. } => format!("sound encode for {who}"),
        _ => format!("{} for {who}", node.id),
    }
}

/// The lowest (most important) rung any request the node serves sits at.
fn kind_of(node: &Node, rungs: &Rungs) -> Kind {
    let index = node.serves.iter().filter_map(|r| rungs.get(r)).min().copied().unwrap_or(0);
    match node.kind {
        NodeKind::Encode { .. } => Kind::Rung { index },
        _ => Kind::Other,
    }
}

/// A ticket for one node, or the refusal that stops the whole change.
pub fn admit(governor: &Governor, node: &Node, rungs: &Rungs, asked: &HashMap<String, RenditionRequest>) -> Result<Option<Ticket>, Refusal> {
    let what = describe(node);
    let kind = kind_of(node, rungs);
    let answer = match &node.kind {
        NodeKind::Encode { shape, encoder, .. } => governor.admit_encode(encoder, shape, &what, kind),
        _ if node.cost.cpu_millicores == 0 && node.cost.device_millis == 0 => return Ok(None),
        _ => governor.admit_claim(Claim::new(&what, node.cost).kind(kind)),
    };
    match answer {
        Admit::Granted(t) => Ok(Some(t)),
        Admit::Refused { need, have, advice } => {
            let request = node
                .serves
                .iter()
                .find_map(|r| asked.get(r))
                .cloned()
                .unwrap_or_default();
            Err(Refusal::governor(&what, need, have, &advice, &request))
        }
    }
}
