//! One channel's destinations planned together: which stream each reads,
//! the smallest graph that serves every one of them, and a refusal that
//! says why for any the planner cannot serve.
//!
//! Every destination of one stream goes into one call of the planner, so
//! three that want 720p share one decode, one scale and one encoder, and one
//! that wants what the stream already is gets a copy.

use std::collections::BTreeMap;

use godwinmix_protocol::destination::DestinationRefusal;
use godwinmix_protocol::rendition::{RenditionRequest, StreamInfo};
use godwinmix_render::{CostModel, Plan};

use super::outcome::{describe, Outcome};
use super::refuse::{from_plan_error, request_of, undecodable};
pub use super::refuse::refusal;

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
