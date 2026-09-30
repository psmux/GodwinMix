//! What the channels' renditions hold between plans, and the replan that
//! moves it on.

use std::collections::BTreeMap;
use std::time::Instant;

use godwinmix_govern::headroom::UNLIMITED;
use godwinmix_govern::Governor;
use godwinmix_protocol::destination::{Carriage, DestinationRefusal, StoredDestination};
use godwinmix_protocol::rendition::{Container, RenditionRequest, StreamInfo};
use godwinmix_render::{Plan, Room};
use serde_json::{json, Value};

use super::admit::{self, Held};
use super::model::Model;
use super::outcome::Outcome;
use super::plan::{self, Stream, Want};
use super::{source, spec, Transcode};
use crate::channels::{Live, Record};

#[derive(Default)]
pub struct State {
    pub channels: BTreeMap<String, ChannelState>,
    /// Destinations the governor shed, by channel and destination: why, and when.
    pub shed: BTreeMap<(String, String), (String, Instant)>,
    /// When refusals were last asked about again.
    pub retried: Option<Instant>,
}

#[derive(Default)]
pub struct ChannelState {
    /// Each live stream as it was planned against. A new bit rate alone
    /// does not replace it, so a plan does not flap between copy and encode
    /// as the rate moves.
    pub snapshots: BTreeMap<String, StreamInfo>,
    pub held: BTreeMap<String, Held>,
    pub outcomes: BTreeMap<String, Outcome>,
    /// What the listener builds, one entry per stream.
    pub streams: Vec<Value>,
    pub plan: Plan,
}

impl State {
    pub fn outcome(&self, channel: &str, destination: &str) -> Option<&Outcome> {
        self.channels.get(channel)?.outcomes.get(destination)
    }
}

/// The destinations of a channel that asked for a rendition and are on.
fn wants(stored: &[StoredDestination]) -> Vec<Want> {
    stored
        .iter()
        .filter(|d| d.enabled)
        .filter_map(|d| {
            let mut request = super::request_for(&d.id, d.rendition.as_ref()?).ok()??;
            request.container = container_for(d);
            Some(Want { id: d.id.clone(), stream: d.stream.clone(), request })
        })
        .collect()
}

/// RTMP carries FLV and SRT carries MPEG-TS, whatever a request says.
fn container_for(d: &StoredDestination) -> Container {
    let srt = godwinmix_protocol::destination::platform(&d.platform).map(|p| p.carriage == Carriage::Srt);
    match srt.unwrap_or_else(|| d.url().starts_with("srt://")) {
        true => Container::MpegTs,
        false => Container::Flv,
    }
}

impl Transcode {
    /// Plan every channel again from its records and its live streams, and
    /// hold a ticket for every node that costs something.
    pub fn replan(&self, records: &[Record], live: &[Live], stored: impl Fn(&Record) -> Vec<StoredDestination>) {
        let mut state = self.state.lock();
        let mut seen = Vec::new();
        for r in records.iter().filter(|r| r.enabled) {
            let wants = wants(&stored(r));
            if wants.is_empty() {
                continue;
            }
            seen.push(r.id.clone());
            let ch = state.channels.entry(r.id.clone()).or_default();
            let streams = streams(&r.id, live, &mut ch.snapshots);
            let held_out = shed_refusals(&state.shed, &r.id);
            let ch = state.channels.get_mut(&r.id).expect("just made");
            self.plan_channel(&r.id, ch, &wants, &streams, held_out);
        }
        state.channels.retain(|id, _| seen.contains(id));
        state.shed.retain(|(c, _), _| seen.contains(c));
    }

    fn plan_channel(&self, id: &str, ch: &mut ChannelState, wants: &[Want], streams: &[Stream], mut held_out: BTreeMap<String, DestinationRefusal>) {
        let gov = self.governor.get();
        let requests: BTreeMap<String, RenditionRequest> = wants.iter().map(|w| (w.id.clone(), w.request.clone())).collect();
        let planned = loop {
            let model = Model::new(self.machine(), gov.profile(), rooms(&gov, &ch.held, self.machine()));
            let planned = plan::plan(wants, streams, &model, &held_out);
            match admit::admit(&gov, id, &planned.plan, &mut ch.held, &requests) {
                Ok(()) => break planned,
                Err(mut no) => {
                    if no.refusal.advice.is_empty() {
                        let base = no.requests.first().and_then(|r| requests.get(r));
                        admit::fill_advice(&mut no.refusal, &model, base);
                    }
                    for r in no.requests {
                        held_out.insert(r, no.refusal.clone());
                    }
                }
            }
        };
        ch.streams = planned
            .sources
            .iter()
            .filter_map(|(name, info)| {
                let nodes = spec::stream_nodes(&planned.plan, name, info, self.machine());
                (!nodes.is_empty()).then(|| json!({"stream": name, "nodes": nodes}))
            })
            .collect();
        ch.outcomes = planned.outcomes;
        ch.plan = planned.plan;
    }
}

/// The channel's live streams, each with the shape it is planned against.
fn streams(channel: &str, live: &[Live], snapshots: &mut BTreeMap<String, StreamInfo>) -> Vec<Stream> {
    let mine: Vec<&Live> = live.iter().filter(|l| l.channel == channel && l.state == "live").collect();
    snapshots.retain(|name, _| mine.iter().any(|l| &l.name == name));
    mine.iter()
        .map(|l| {
            let now = source::info(l);
            let info = match (now, snapshots.get(&l.name)) {
                (Some(now), Some(was)) if source::same_shape(&now, was) => Some(was.clone()),
                (Some(now), _) => {
                    snapshots.insert(l.name.clone(), now.clone());
                    Some(now)
                }
                (None, _) => None,
            };
            Stream { name: l.name.clone(), since_ms: l.since_ms, info }
        })
        .collect()
}

fn shed_refusals(shed: &BTreeMap<(String, String), (String, Instant)>, channel: &str) -> BTreeMap<String, DestinationRefusal> {
    shed.iter()
        .filter(|((c, _), _)| c == channel)
        .map(|((_, d), (why, _))| (d.clone(), plan::refusal("shed", why.clone())))
        .collect()
}

/// What is left on each hardware device for this channel's plan, with what
/// the channel already holds there given back, so a replan does not find the
/// device full of its own encoders.
fn rooms(gov: &Governor, held: &BTreeMap<String, Held>, machine: &super::Machine) -> BTreeMap<String, Room> {
    let mut out = BTreeMap::new();
    for device in machine.slots().into_iter().filter_map(|s| s.device) {
        let have = gov.headroom(Some(&device));
        let mine = held.values().filter(|h| h.node.device == device).fold((0, 0), |(m, s), h| {
            (m + h.ticket.cost().device_millis, s + h.ticket.cost().device_sessions)
        });
        let sessions = (have.device_sessions != UNLIMITED).then(|| have.device_sessions + mine.1);
        out.insert(device, Room { sessions, device_millis: Some(have.device_millis + mine.0) });
    }
    out
}
