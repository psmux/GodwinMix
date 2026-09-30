//! A plan as the page sees it, and one request's rung as a consumer sees it.

use super::graph::Graph;
use super::Tap;
use godwinmix_protocol::rendition::{
    DeviceTotal, PlanNode, PlanReason, PlanTotals, PlanView, RenditionRequest,
};
use godwinmix_render::{Node, NodeKind, Plan, ReasonCode};
use std::collections::BTreeMap;
use std::time::Instant;

fn kind_slug(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Source { .. } => "source",
        NodeKind::Copy { .. } => "copy",
        NodeKind::Decode { .. } => "decode",
        NodeKind::Scale { .. } => "scale",
        NodeKind::Encode { .. } => "encode",
        NodeKind::AudioConvert { .. } => "audio-convert",
        NodeKind::AudioEncode { .. } => "audio-encode",
        NodeKind::Mux { .. } => "mux",
    }
}

fn code_slug(code: ReasonCode) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Request id to the output that made it.
pub type Owners = std::collections::HashMap<String, String>;

fn node_view(n: &Node, shed: &BTreeMap<String, (String, Instant)>, owners: &Owners) -> PlanNode {
    let mut serves: Vec<String> = Vec::new();
    for r in &n.serves {
        let owner = owners.get(r).cloned().unwrap_or_else(|| r.clone());
        if !serves.contains(&owner) {
            serves.push(owner);
        }
    }
    PlanNode {
        id: n.id.clone(),
        kind: kind_slug(&n.kind).into(),
        serves,
        encoder: match &n.kind {
            NodeKind::Encode { encoder, .. } => Some(encoder.id.clone()),
            _ => None,
        },
        reason: n.reason.as_ref().map(|r| PlanReason { code: code_slug(r.code), text: r.text.clone() }),
        cost: n.cost,
        shed: shed.get(&n.id).map(|(why, _)| why.clone()),
    }
}

pub fn of(plan: &Plan, shed: &BTreeMap<String, (String, Instant)>, owners: &Owners) -> PlanView {
    let mut devices = BTreeMap::new();
    for (device, cost) in plan.cost.iter().filter(|(d, _)| d.as_str() != godwinmix_render::CPU) {
        devices.insert(device.clone(), DeviceTotal { millis: cost.device_millis, sessions: cost.device_sessions });
    }
    PlanView {
        nodes: plan.nodes.iter().map(|n| node_view(n, shed, owners)).collect(),
        totals: PlanTotals {
            cpu_millicores: plan.total.cpu_millicores,
            devices,
            egress_kbps: plan.total.egress_kbps,
        },
    }
}

/// One request's rung: the outlets of the encodes its Mux reads.
pub fn tap(plan: &Plan, graph: &Graph, request: &RenditionRequest, rung: usize) -> Tap {
    let mut tap = Tap {
        request: request.id.clone(),
        rung,
        video: None,
        audio: None,
        keyframe_ms: plan.keyframe_ms.values().next().copied().unwrap_or(godwinmix_render::DEFAULT_KEYFRAME_MS),
        video_tee: None,
        audio_tee: None,
        program: graph.pipeline().clone(),
    };
    let Some(mux) = plan.output(&request.id) else { return tap };
    for input in &mux.inputs {
        match plan.node(input).map(|n| &n.kind) {
            Some(NodeKind::Encode { shape, .. }) => {
                tap.video = Some(*shape);
                tap.video_tee = graph.outlet(input);
            }
            Some(NodeKind::AudioEncode { shape, .. }) => {
                tap.audio = Some(*shape);
                tap.audio_tee = graph.outlet(input);
            }
            _ => {}
        }
    }
    tap
}
