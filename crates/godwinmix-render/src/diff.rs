//! What changes between two plans, so a running graph touches only that.

use std::collections::HashMap;

use serde::Serialize;

use crate::graph::{Node, Plan};

/// Node ids to act on. Apply in this order: `stop`, then `restart`, then
/// `start`. Each list is already in the order to act on it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PlanDiff {
    /// In the new plan only. Producers first.
    pub start: Vec<String>,
    /// In the old plan only. Consumers first, so nothing reads a stopped node.
    pub stop: Vec<String>,
    /// In both under one id, but doing different work or reading different
    /// inputs (an encoder moved off a full GPU, a Mux that now reads an
    /// encode instead of a copy). Producers first.
    pub restart: Vec<String>,
    /// In both and unchanged. Which requests they serve may differ; that is
    /// a new reader on the same output, not a change to the node.
    pub keep: Vec<String>,
}

impl PlanDiff {
    /// True when applying it would change nothing that runs.
    pub fn is_empty(&self) -> bool {
        self.start.is_empty() && self.stop.is_empty() && self.restart.is_empty()
    }
}

/// Compares two plans node by node, by id.
pub fn diff(old: &Plan, new: &Plan) -> PlanDiff {
    let before: HashMap<&str, &Node> = old.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let after: HashMap<&str, &Node> = new.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut out = PlanDiff::default();
    for node in &new.nodes {
        match before.get(node.id.as_str()) {
            None => out.start.push(node.id.clone()),
            Some(was) if same_work(was, node) => out.keep.push(node.id.clone()),
            Some(_) => out.restart.push(node.id.clone()),
        }
    }
    for node in old.nodes.iter().rev() {
        if !after.contains_key(node.id.as_str()) {
            out.stop.push(node.id.clone());
        }
    }
    out
}

fn same_work(a: &Node, b: &Node) -> bool {
    a.kind == b.kind && a.inputs == b.inputs
}
