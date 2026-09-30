//! What the running renditions answer about themselves: the plan as the
//! page sees it, the shed list, each output's rungs, and what the plan holds
//! on each device.

use super::renditions::Renditions;
use super::{view, Tap};
use godwinmix_protocol::rendition::{Cost, ShedNote};
use std::collections::BTreeMap;

impl Renditions {
    /// The request ids one output made.
    pub fn requests_of(&self, output: &str) -> Vec<String> {
        self.outputs
            .iter()
            .filter(|(o, _)| o == output)
            .flat_map(|(_, rs)| rs.iter().map(|r| r.id.clone()))
            .collect()
    }

    /// What the running plan holds on each hardware device.
    pub(super) fn held(&self) -> BTreeMap<String, Cost> {
        let mut out: BTreeMap<String, Cost> = BTreeMap::new();
        for (id, t) in &self.tickets {
            if let Some(node) = self.plan.node(id).filter(|n| n.device != godwinmix_render::CPU) {
                let e = out.entry(node.device.clone()).or_default();
                *e = e.plus(t.cost());
            }
        }
        out
    }

    pub(super) fn publish(&self) {
        let owners: view::Owners = self
            .outputs
            .iter()
            .flat_map(|(o, rs)| rs.iter().map(move |r| (r.id.clone(), o.clone())))
            .collect();
        *self.shared.write() = view::of(&self.plan, &self.shed, &owners);
        *self.notes.write() = self
            .shed
            .iter()
            .filter_map(|(id, (why, _))| {
                let node = self.plan.node(id)?;
                Some(ShedNote { what: super::admit::describe(node), why: why.clone() })
            })
            .collect();
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
}
