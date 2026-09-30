//! One channel's destinations planned together: which stream each reads,
//! the smallest graph that serves every one of them, and a refusal that
//! says why for any the planner cannot serve.
//!
//! Every destination of one stream goes into one call of the planner, so
//! three that want 720p share one decode, one scale and one encoder, and one
//! that wants what the stream already is gets a copy.

use std::collections::BTreeMap;

use godwinmix_protocol::destination::DestinationRefusal;
use godwinmix_protocol::rendition::{AudioCodec, RenditionAdvice, RenditionRequest, StreamInfo, VideoCodec};
use godwinmix_render::{CostModel, NodeKind, Plan, PlanError, Track};

use super::outcome::{describe, Outcome};
use super::source::video_codec;

/// A destination that asked for a rendition.
#[derive(Debug, Clone)]
pub struct Want {
    pub id: String,
    /// As configured: a stream name, or `*` for the one live longest.
    pub stream: String,
    /// The request, its id the destination's and its container the one the
    /// destination's carriage uses.
    pub request: RenditionRequest,
}

/// A live stream of the channel.
#[derive(Debug, Clone)]
pub struct Stream {
    pub name: String,
    pub since_ms: u64,
    /// `None` while its codecs are still arriving.
    pub info: Option<StreamInfo>,
}

/// What planning one channel came to.
#[derive(Debug, Default)]
pub struct Planned {
    pub plan: Plan,
    pub sources: Vec<(String, StreamInfo)>,
    pub outcomes: BTreeMap<String, Outcome>,
}

/// The stream a destination reads now, by the rule the listener uses for
/// `*`: the one that has been live longest.
pub fn pick<'a>(wanted: &str, streams: &'a [Stream]) -> Option<&'a Stream> {
    if wanted != "*" {
        return streams.iter().find(|s| s.name == wanted);
    }
    streams.iter().min_by_key(|s| (s.since_ms, s.name.clone()))
}

/// Plan every want that is not held out. A held out one (refused by the
/// governor, or shed) keeps its refusal.
pub fn plan(wants: &[Want], streams: &[Stream], model: &dyn CostModel, held_out: &BTreeMap<String, DestinationRefusal>) -> Planned {
    let mut out = Planned::default();
    let mut requests: Vec<(String, RenditionRequest)> = Vec::new();
    for w in wants {
        if let Some(no) = held_out.get(&w.id) {
            out.outcomes.insert(w.id.clone(), Outcome::Refused(no.clone()));
            continue;
        }
        let Some((stream, info)) = pick(&w.stream, streams).and_then(|s| Some((s, s.info.clone()?))) else {
            out.outcomes.insert(w.id.clone(), Outcome::Waiting);
            continue;
        };
        if !out.sources.iter().any(|(n, _)| n == &stream.name) {
            out.sources.push((stream.name.clone(), info));
        }
        requests.push((stream.name.clone(), w.request.clone()));
    }
    loop {
        match godwinmix_render::plan(&out.sources, &requests, model) {
            Ok(p) => match undecodable(&p, &out.sources) {
                None => {
                    out.plan = p;
                    break;
                }
                Some((served, no)) => refuse(&mut out, &mut requests, &served, &no),
            },
            Err(e) => {
                let no = from_plan_error(&e, &requests);
                refuse(&mut out, &mut requests, &[request_of(&e)], &no);
            }
        }
    }
    for (stream, request) in &requests {
        let info = out.sources.iter().find(|(n, _)| n == stream).map(|(_, i)| i.clone()).unwrap_or_default();
        out.outcomes.insert(request.id.clone(), describe(&out.plan, &request.id, stream, &info));
    }
    out
}

fn refuse(out: &mut Planned, requests: &mut Vec<(String, RenditionRequest)>, ids: &[String], no: &DestinationRefusal) {
    requests.retain(|(_, r)| !ids.contains(&r.id));
    for id in ids {
        out.outcomes.insert(id.clone(), Outcome::Refused(no.clone()));
    }
}

/// The listener decodes H.264 and AAC, which is what RTMP carries. A plan
/// that would decode anything else is refused for the destinations it
/// serves, with the reason.
fn undecodable(plan: &Plan, sources: &[(String, StreamInfo)]) -> Option<(Vec<String>, DestinationRefusal)> {
    for n in &plan.nodes {
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

fn request_of(e: &PlanError) -> String {
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
fn from_plan_error(e: &PlanError, requests: &[(String, RenditionRequest)]) -> DestinationRefusal {
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
