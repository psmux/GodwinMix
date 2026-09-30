//! What a client is answered: a channel's plan in the shape
//! `rendition.plan` gives the programme's, and a governor refusal as the
//! Safety error a programme output's refusal is.

use std::collections::BTreeMap;

use godwinmix_protocol::destination::StoredDestination;
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::rendition::{DeviceTotal, PlanNode, PlanReason, PlanTotals, PlanView, RenditionRefusal};
use godwinmix_render::{Node, NodeKind, Plan, CPU};

use super::Transcode;

impl Transcode {
    /// `rendition.plan {scope: "channel:<id>"}`: every node of the channel's
    /// plan, each naming the destinations it `serves` and, for an encode,
    /// the catalogue id of its encoder. A node the governor shed says why.
    /// `None` for a channel that converts nothing.
    pub fn plan_view(&self, channel: &str) -> Option<PlanView> {
        let state = self.state.lock();
        let ch = state.channels.get(channel)?;
        let shed: BTreeMap<&str, &str> = state
            .shed
            .iter()
            .filter(|((c, _), _)| c == channel)
            .map(|((_, d), (why, _))| (d.as_str(), why.as_str()))
            .collect();
        Some(view(&ch.plan, &shed))
    }

    /// The governor's refusal of a destination as the Safety error a client
    /// gets, `data: {need, have, advice: [{text, request}]}`. `None` unless
    /// the governor refused it.
    pub fn refusal_error(&self, channel: &str, d: &StoredDestination) -> Option<RpcError> {
        let (_, no) = self.view(channel, &d.id);
        let no = no.filter(|n| n.code == "governor" && d.enabled && d.rendition.is_some())?;
        let data = RenditionRefusal { need: no.need.unwrap_or_default(), have: no.have.unwrap_or_default(), advice: no.advice };
        let mut e = RpcError::new(ErrorCode::Safety, format!("Destination `{}` was not saved: {}", d.id, no.message));
        if let Ok(serde_json::Value::Object(map)) = serde_json::to_value(data) {
            e = e.with_data(map);
        }
        Some(e.with("destination", d.id.clone()).with("channel", channel))
    }
}

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

fn node_view(n: &Node, shed: &BTreeMap<&str, &str>) -> PlanNode {
    let code = |c| serde_json::to_value(c).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
    PlanNode {
        id: n.id.clone(),
        kind: kind_slug(&n.kind).into(),
        serves: n.serves.clone(),
        encoder: match &n.kind {
            NodeKind::Encode { encoder, .. } => Some(encoder.id.clone()),
            _ => None,
        },
        reason: n.reason.as_ref().map(|r| PlanReason { code: code(r.code), text: r.text.clone() }),
        cost: n.cost,
        shed: n.serves.iter().find_map(|d| shed.get(d.as_str())).map(|why| why.to_string()),
    }
}

fn view(plan: &Plan, shed: &BTreeMap<&str, &str>) -> PlanView {
    let devices = plan
        .cost
        .iter()
        .filter(|(d, _)| d.as_str() != CPU)
        .map(|(d, c)| (d.clone(), DeviceTotal { millis: c.device_millis, sessions: c.device_sessions }))
        .collect();
    PlanView {
        nodes: plan.nodes.iter().map(|n| node_view(n, shed)).collect(),
        totals: PlanTotals { cpu_millicores: plan.total.cpu_millicores, devices, egress_kbps: plan.total.egress_kbps },
    }
}
