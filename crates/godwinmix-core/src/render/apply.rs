//! One change to the programme's renditions, planned whole, admitted whole
//! and then applied: a refusal leaves everything as it was.

use super::admit::{admit, Rungs};
use super::model::GovernorModel;
use super::refusal::Refusal;
use super::renditions::{Renditions, PROGRAMME};
use godwinmix_govern::Ticket;
use godwinmix_protocol::rendition::RenditionRequest;
use godwinmix_render::{diff, plan};
use std::collections::HashMap;
use tracing::info;

impl Renditions {
    pub(super) fn apply(&mut self, next: Vec<(String, Vec<RenditionRequest>)>) -> Result<(), Refusal> {
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
}

fn rungs(outputs: &[(String, Vec<RenditionRequest>)]) -> Rungs {
    outputs
        .iter()
        .flat_map(|(_, rs)| rs.iter().enumerate().map(|(i, r)| (r.id.clone(), i.min(255) as u8)))
        .collect()
}
