//! The programme's renditions: which outputs asked for what, the plan that
//! serves them all, the tickets that plan holds, and the running graph.
//!
//! A change is planned whole, admitted whole and then applied: a refusal
//! from the governor or the planner leaves everything as it was.

use super::admit::{admit, Rungs};
use super::graph::{Graph, Programme};
use super::model::GovernorModel;
use super::refusal::Refusal;
use super::{view, Station, Tap};
use godwinmix_govern::Ticket;
use godwinmix_protocol::rendition::{PlanView, RenditionChoice, RenditionRequest, StreamInfo};
use godwinmix_render::{diff, plan, presets, Plan};
use parking_lot::RwLock;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tracing::info;

/// The one source the programme's renditions read.
pub const PROGRAMME: &str = "programme";

pub struct Renditions {
    pub(super) station: Station,
    pub(super) graph: Graph,
    source: StreamInfo,
    /// Output id to its requests, top rung first, in the order added.
    outputs: Vec<(String, Vec<RenditionRequest>)>,
    pub(super) plan: Plan,
    pub(super) tickets: HashMap<String, Ticket>,
    /// Node id to why the governor stopped it, and when.
    pub(super) shed: BTreeMap<String, (String, std::time::Instant)>,
    pub(super) shared: Arc<RwLock<PlanView>>,
    pub(super) rungs: Rungs,
}

impl Renditions {
    pub fn new(station: Station, programme: Programme, source: StreamInfo) -> Self {
        Renditions {
            station,
            graph: Graph::new(programme),
            source,
            outputs: Vec::new(),
            plan: Plan::default(),
            tickets: HashMap::new(),
            shed: BTreeMap::new(),
            shared: Arc::new(RwLock::new(PlanView::default())),
            rungs: Rungs::new(),
        }
    }

    /// A new station, before anything is planned.
    pub fn set_station(&mut self, station: Station) {
        self.station = station;
    }

    pub fn station(&self) -> &Station {
        &self.station
    }

    /// What `rendition.plan` reads, from any thread.
    pub fn shared_view(&self) -> Arc<RwLock<PlanView>> {
        self.shared.clone()
    }

    /// Plan `output` in with everything else and start what it needs.
    /// `Ok(None)` means it asked for no conversion (the `copy` preset) and
    /// reads the programme encoder like an output with no rendition.
    pub fn add(&mut self, output: &str, choice: &RenditionChoice) -> Result<Option<Vec<Tap>>, Refusal> {
        let Some(requests) = presets::expand(output, choice).map_err(Refusal::bad_choice)? else {
            return Ok(None);
        };
        let mut next = self.outputs.clone();
        next.retain(|(o, _)| o != output);
        next.push((output.to_string(), requests));
        self.apply(next)?;
        Ok(Some(self.taps(output)))
    }

    /// Take `output` out; stops only what nothing else uses.
    pub fn remove(&mut self, output: &str) {
        if !self.outputs.iter().any(|(o, _)| o == output) {
            return;
        }
        let mut next = self.outputs.clone();
        next.retain(|(o, _)| o != output);
        // Removing never needs a ticket, so it cannot be refused.
        let _ = self.apply(next);
    }

    /// The request ids one output made.
    pub fn requests_of(&self, output: &str) -> Vec<String> {
        self.outputs
            .iter()
            .filter(|(o, _)| o == output)
            .flat_map(|(_, rs)| rs.iter().map(|r| r.id.clone()))
            .collect()
    }

    pub fn has(&self, output: &str) -> bool {
        self.outputs.iter().any(|(o, _)| o == output)
    }

    fn apply(&mut self, next: Vec<(String, Vec<RenditionRequest>)>) -> Result<(), Refusal> {
        let requests: Vec<(String, RenditionRequest)> = next
            .iter()
            .flat_map(|(_, rs)| rs.iter().map(|r| (PROGRAMME.to_string(), r.clone())))
            .collect();
        let model = GovernorModel::new(self.station.governor().clone(), self.station.slots(), self.station.audio(), self.held());
        let sources = [(PROGRAMME.to_string(), self.source.clone())];
        let new = plan(&sources, &requests, &model).map_err(|e| Refusal::plan(&e))?;
        let d = diff(&self.plan, &new);
        let rungs = rungs(&next);
        let asked: HashMap<String, RenditionRequest> = requests.into_iter().map(|(_, r)| (r.id.clone(), r)).collect();
        // Every new or changed node is admitted before anything is touched.
        let mut granted: HashMap<String, Ticket> = HashMap::new();
        // Encoders first, so a refusal names the rendition rather than the
        // scaling in front of it.
        let mut order: Vec<&String> = d.start.iter().chain(&d.restart).collect();
        order.sort_by_key(|id| !id.starts_with("encode:"));
        for id in order {
            let node = new.node(id).expect("the diff names nodes of the new plan");
            if let Some(t) = admit(self.station.governor(), node, &rungs, &asked)? {
                granted.insert(id.clone(), t);
            }
        }
        for id in &d.stop {
            self.graph.stop(id);
            self.tickets.remove(id);
            self.shed.remove(id);
        }
        for id in &d.restart {
            self.graph.suspend(id);
            self.tickets.remove(id);
            self.shed.remove(id);
        }
        let cat = crate::catalogue::global();
        for node in new.nodes.iter().filter(|n| d.start.contains(&n.id) || d.restart.contains(&n.id)) {
            let ticket = granted.remove(&node.id);
            let preset = ticket.as_ref().and_then(|t| t.preset().map(str::to_string));
            if let Err(e) = self.graph.start(node, &new, &cat, preset.as_deref()) {
                tracing::error!(node = %node.id, error = %format!("{e:#}"), "a rendition node would not start");
            }
            if let Some(t) = ticket {
                self.tickets.insert(node.id.clone(), t);
            }
        }
        info!(start = d.start.len(), stop = d.stop.len(), restart = d.restart.len(), keep = d.keep.len(), "renditions replanned");
        self.outputs = next;
        self.rungs = rungs;
        self.plan = new;
        self.publish();
        Ok(())
    }

    /// What the running plan holds on each hardware device.
    fn held(&self) -> BTreeMap<String, godwinmix_protocol::rendition::Cost> {
        let mut out: BTreeMap<String, godwinmix_protocol::rendition::Cost> = BTreeMap::new();
        for (id, t) in &self.tickets {
            if let Some(node) = self.plan.node(id).filter(|n| n.device != godwinmix_render::CPU) {
                let e = out.entry(node.device.clone()).or_default();
                *e = e.plus(t.cost());
            }
        }
        out
    }

    pub(super) fn publish(&self) {
        *self.shared.write() = view::of(&self.plan, &self.shed);
    }

    /// The rungs of one output, top first.
    pub fn taps(&self, output: &str) -> Vec<Tap> {
        let Some((_, requests)) = self.outputs.iter().find(|(o, _)| o == output) else { return Vec::new() };
        requests
            .iter()
            .enumerate()
            .map(|(rung, r)| view::tap(&self.plan, &self.graph, r, rung))
            .collect()
    }

    pub fn shutdown(&mut self) {
        self.graph.stop_all();
        self.tickets.clear();
    }
}

fn rungs(outputs: &[(String, Vec<RenditionRequest>)]) -> Rungs {
    outputs
        .iter()
        .flat_map(|(_, rs)| rs.iter().enumerate().map(|(i, r)| (r.id.clone(), i.min(255) as u8)))
        .collect()
}
