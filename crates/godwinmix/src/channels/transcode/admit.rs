//! Asking the governor before any of it runs.
//!
//! Every node that costs CPU or a hardware device holds a ticket for as long
//! as it is in the plan. A node that stays in the plan unchanged keeps the
//! ticket it has, so replanning a running channel never asks again for what
//! is already on air. Copies and muxes hold nothing: a copy is free beyond
//! the network, and the mux is the restreamer, which a destination that asked
//! for nothing runs as well.

use std::collections::BTreeMap;

use godwinmix_govern::{Admit, Advice, Claim, Governor, Kind, Ticket};
use godwinmix_protocol::destination::DestinationRefusal;
use godwinmix_protocol::rendition::{Cost, EncoderSlot, Fps, RenditionAdvice, RenditionRequest, VideoCodec, VideoShape};
use godwinmix_render::container::{audio_name, shape_text, video_name};
use godwinmix_render::{CostModel, Node, NodeKind, Plan, CPU};

/// A node the governor let in, and the share it holds.
pub struct Held {
    pub node: Node,
    pub ticket: Ticket,
}

/// A node the governor turned away, and the destinations it would have served.
pub struct Refused {
    pub requests: Vec<String>,
    pub refusal: DestinationRefusal,
}

/// Channel transcodes are shed before a show's own renditions and after
/// previews: the rank of a lower rung of a ladder, which the governor may
/// drop, where it never drops the top one.
pub const KIND: Kind = Kind::Rung { index: 1 };

fn costs(n: &Node) -> bool {
    !matches!(n.kind, NodeKind::Source { .. } | NodeKind::Copy { .. } | NodeKind::Mux { .. })
}

/// Hold a ticket for every node of `plan` that costs something. Tickets for
/// nodes the plan no longer has, or whose work changed, are given back
/// first, so a swap from 720p to 480p is judged without the 720p encoder.
pub fn admit(
    gov: &Governor,
    channel: &str,
    plan: &Plan,
    held: &mut BTreeMap<String, Held>,
    requests: &BTreeMap<String, RenditionRequest>,
) -> Result<(), Box<Refused>> {
    held.retain(|id, h| plan.node(id).is_some_and(|n| n.kind == h.node.kind && n.inputs == h.node.inputs));
    for n in plan.nodes.iter().filter(|n| costs(n)) {
        if held.contains_key(&n.id) {
            continue;
        }
        let mut claim = Claim::new(&what(channel, n), n.cost).kind(KIND);
        if n.device != CPU {
            claim = claim.on(&n.device);
        }
        match gov.admit_claim(claim) {
            Admit::Granted(ticket) => {
                held.insert(n.id.clone(), Held { node: n.clone(), ticket });
            }
            Admit::Refused { need, have, advice } => {
                let base = n.serves.first().and_then(|id| requests.get(id));
                return Err(Box::new(Refused { requests: n.serves.clone(), refusal: refusal(need, have, &advice, base) }));
            }
        }
    }
    Ok(())
}

/// The governor's refusal as a destination shows it, each shape that would
/// fit made into a request the page can retry with.
pub fn refusal(need: Cost, have: Cost, advice: &Advice, base: Option<&RenditionRequest>) -> DestinationRefusal {
    let buttons = advice
        .fits
        .iter()
        .filter_map(|fit| {
            let mut request = base?.clone();
            let video = request.video.get_or_insert_with(Default::default);
            video.codec = Some(fit.slot.codec);
            (video.width, video.height, video.fps) = (Some(fit.width), Some(fit.height), Some(fit.fps));
            Some(RenditionAdvice { text: fit.label.clone(), request })
        })
        .collect();
    DestinationRefusal {
        code: "governor".into(),
        message: advice.text.clone(),
        need: Some(need),
        have: Some(have),
        advice: buttons,
    }
}

/// What a node is, in words, for the governor's sentence and its alerts.
pub fn what(channel: &str, n: &Node) -> String {
    match &n.kind {
        NodeKind::Encode { shape, encoder, .. } => format!(
            "the {} {} encode on {} for channel `{channel}`",
            video_name(shape.codec),
            shape_text(shape),
            encoder.id
        ),
        NodeKind::Decode { source, .. } => format!("decoding stream `{source}` of channel `{channel}`"),
        NodeKind::Scale { width, height, .. } => format!("scaling to {width}x{height} for channel `{channel}`"),
        NodeKind::AudioEncode { shape, .. } => {
            format!("the {} sound encode for channel `{channel}`", audio_name(shape.codec))
        }
        _ => format!("converting sound for channel `{channel}`"),
    }
}

/// Shapes offered when the governor has none of its own to offer, which is
/// what an uncalibrated machine gets: the largest first.
const SHAPES: [(u32, u32, u32); 4] = [(1920, 1080, 30), (1280, 720, 30), (854, 480, 30), (640, 360, 30)];

/// The governor's own closing sentence when it has nothing to suggest.
const NOTHING: &str = " Nothing more fits now: stop a preview or an output, or make one that is running smaller.";

/// Fill a refusal's buttons from the plan's own prices, when the governor's
/// profile knows no encoder to suggest: each standard shape whose encode
/// fits in what is free, on the first encoder of the codec that can make it.
pub fn fill_advice(no: &mut DestinationRefusal, model: &dyn CostModel, base: Option<&RenditionRequest>) {
    let (Some(have), Some(base)) = (no.have, base) else { return };
    let codec = base.video.as_ref().and_then(|v| v.codec).unwrap_or(VideoCodec::H264);
    let slots: Vec<EncoderSlot> = model.encoders().into_iter().filter(|s| s.codec == codec).collect();
    for (width, height, fps) in SHAPES {
        let shape = VideoShape { codec, width, height, fps: Fps::whole(fps), bitrate_kbps: 0, keyframe_ms: 0 };
        let cheap = |s: &&EncoderSlot| model.encode_cost(&shape, s).is_some_and(|c| c.cpu_millicores <= have.cpu_millicores);
        let Some(slot) = slots.iter().find(cheap) else { continue };
        let mut request = base.clone();
        let video = request.video.get_or_insert_with(Default::default);
        (video.width, video.height, video.fps) = (Some(width), Some(height), Some(Fps::whole(fps)));
        let text = format!("{height}p{fps} {} on {}", video_name(codec), slot.id);
        no.advice.push(RenditionAdvice { text, request });
    }
    if let Some(first) = no.advice.first() {
        let said = no.message.trim_end_matches(NOTHING).to_string();
        no.message = format!("{said} {} fits.", first.text);
    }
}
