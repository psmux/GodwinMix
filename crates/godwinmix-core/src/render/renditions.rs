//! The programme's renditions: which outputs asked for what, the plan that
//! serves them all, the tickets that plan holds, and the running graph.
//!
//! A change is planned whole, admitted whole and then applied: a refusal
//! from the governor or the planner leaves everything as it was.

use super::admit::Rungs;
use super::graph::{Graph, Programme};
use super::refusal::Refusal;
use super::{Station, Tap};
use godwinmix_govern::Ticket;
use godwinmix_protocol::rendition::{PlanView, RenditionChoice, RenditionRequest, ShedNote, StreamInfo};
use godwinmix_render::{presets, Plan};
use parking_lot::RwLock;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// The one source the programme's renditions read.
pub const PROGRAMME: &str = "programme";

pub struct Renditions {
    pub(super) station: Station,
    pub(super) graph: Graph,
    pub(super) source: StreamInfo,
    /// Output id to its requests, top rung first, in the order added.
    pub(super) outputs: Vec<(String, Vec<RenditionRequest>)>,
    pub(super) plan: Plan,
    pub(super) tickets: HashMap<String, Ticket>,
    /// Node id to why the governor stopped it, and when.
    pub(super) shed: BTreeMap<String, (String, std::time::Instant)>,
    pub(super) shared: Arc<RwLock<PlanView>>,
    pub(super) notes: Arc<RwLock<Vec<ShedNote>>>,
    pub(super) rungs: Rungs,
}

/// What the control plane reads, from any thread, without asking the mixer.
#[derive(Clone)]
pub struct RenditionsHandle {
    pub station: Station,
    /// The programme as the planner sees it, for pricing presets.
    pub source: StreamInfo,
    pub plan: Arc<RwLock<PlanView>>,
    pub shed: Arc<RwLock<Vec<ShedNote>>>,
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
            notes: Arc::new(RwLock::new(Vec::new())),
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

    pub fn handle(&self) -> RenditionsHandle {
        RenditionsHandle {
            station: self.station.clone(),
            source: self.source.clone(),
            plan: self.shared.clone(),
            shed: self.notes.clone(),
        }
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

    pub fn has(&self, output: &str) -> bool {
        self.outputs.iter().any(|(o, _)| o == output)
    }

    pub fn shutdown(&mut self) {
        self.graph.stop_all();
        self.tickets.clear();
    }
}
