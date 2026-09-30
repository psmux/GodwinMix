//! Why the planner cannot serve a destination, as the refusal it shows.

use godwinmix_protocol::destination::DestinationRefusal;
use godwinmix_protocol::rendition::{AudioCodec, RenditionAdvice, RenditionRequest, StreamInfo, VideoCodec};
use godwinmix_render::{NodeKind, Plan, PlanError, Track};

use super::source::video_codec;

/// The listener decodes H.264 and AAC, which is what RTMP carries, and its
/// senders carry H.264 and AAC (classic RTMP, and MPEG-TS made from it). A
/// plan that would decode or encode anything else is refused for the
/// destinations it serves, with the reason.
pub fn undecodable(plan: &Plan, sources: &[(String, StreamInfo)]) -> Option<(Vec<String>, DestinationRefusal)> {
    for n in &plan.nodes {
        let made = match &n.kind {
            NodeKind::Encode { shape, .. } => (shape.codec != VideoCodec::H264).then_some("video other than H.264"),
            NodeKind::AudioEncode { shape, .. } => (shape.codec != AudioCodec::Aac).then_some("sound other than AAC"),
            _ => None,
        };
        if let Some(what) = made {
            let message = format!(
                "A channel destination sends H.264 video and AAC sound, and this asks for {what}. \
                 Ask for H.264 and AAC, or send the stream as it arrives."
            );
            return Some((n.serves.clone(), refusal("plan", message)));
        }
        let NodeKind::Decode { source, track } = &n.kind else { continue };
        let info = sources.iter().find(|(s, _)| s == source).map(|(_, i)| i)?;
        let (ok, name) = match track {
            Track::Video => (info.video.is_some_and(|v| v.codec == VideoCodec::H264), "its video"),
            Track::Audio => (info.audio.is_some_and(|a| a.codec == AudioCodec::Aac), "its sound"),
        };
        if !ok {
            let message = format!(
                "Stream `{source}` would have to be decoded to make this, and {name} is not H.264 or AAC, \
                 which is all the channel server converts from. Ask for a copy, or send H.264 and AAC."
            );
            return Some((n.serves.clone(), refusal("plan", message)));
        }
    }
    None
}

pub fn refusal(code: &str, message: String) -> DestinationRefusal {
    DestinationRefusal { code: code.into(), message, need: None, have: None, advice: Vec::new() }
}

pub fn request_of(e: &PlanError) -> String {
    match e {
        PlanError::UnknownSource { request, .. }
        | PlanError::DuplicateRequest { request }
        | PlanError::ContainerCodec { request, .. }
        | PlanError::MissingTrack { request, .. }
        | PlanError::NothingAsked { request }
        | PlanError::NoAudioEncoder { request, .. } => request.clone(),
        PlanError::NoEncoder(b) => b.request.clone(),
    }
}

/// A planner refusal, with the nearest shape it found as a button.
pub fn from_plan_error(e: &PlanError, requests: &[(String, RenditionRequest)]) -> DestinationRefusal {
    let mut no = refusal("plan", e.to_string());
    if let PlanError::NoEncoder(b) = e {
        let base = requests.iter().find(|(_, r)| r.id == b.request).map(|(_, r)| r.clone());
        if let (Some(s), Some(mut request)) = (&b.nearest, base) {
            let video = request.video.get_or_insert_with(Default::default);
            video.codec = Some(video_codec(&s.codec));
            (video.width, video.height, video.fps) = (Some(s.width), Some(s.height), Some(s.fps));
            no.advice.push(RenditionAdvice { text: s.text.clone(), request });
        }
    }
    no
}
