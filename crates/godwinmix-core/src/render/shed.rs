//! Live wins. When the governor says the machine is over its line, the
//! encoders it names are stopped (their outlets stay, so every output that
//! read them is still linked), the outputs are marked with why, and once
//! there has been room for a while each one is admitted again and restarted.
//!
//! Only `Drop` steps are acted on. A faster software preset would mean
//! restarting the encoder, which on the top rung is a visible break; that
//! step is logged and left to the next wave.

use super::admit::admit;
use super::renditions::Renditions;
use godwinmix_govern::ShedAction;
use godwinmix_protocol::rendition::ShedNote;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// How long a shed encoder stays down before it is tried again, so a
/// machine on the edge does not flap.
pub const RESTORE_AFTER: Duration = Duration::from_secs(10);

/// What one tick did, for the alerts and `event/governor.shed`.
#[derive(Debug, Default)]
pub struct Tick {
    pub shed: Vec<ShedNote>,
    pub restored: Vec<String>,
}

impl Renditions {
    /// Once a watchdog tick, on the mixer thread: act on the governor's shed
    /// list, and bring back what was shed when there is room again.
    pub fn tick(&mut self) -> Tick {
        let mut out = Tick::default();
        let steps = self.station.governor().shed();
        for step in &steps {
            let Some(id) = self.node_holding(step.ticket) else { continue };
            match &step.action {
                ShedAction::Drop => {
                    self.graph.suspend(&id);
                    self.tickets.remove(&id);
                    warn!(node = %id, why = %step.why, "rendition shed");
                    self.shed.insert(id, (step.why.clone(), Instant::now()));
                    out.shed.push(ShedNote { what: step.what.clone(), why: step.why.clone() });
                }
                ShedAction::LowerPreset { to, .. } => {
                    info!(node = %id, preset = %to, "a faster preset was asked for and is not applied while running");
                }
            }
        }
        if steps.is_empty() {
            out.restored = self.restore();
        }
        if !out.shed.is_empty() || !out.restored.is_empty() {
            self.publish();
        }
        out
    }

    fn node_holding(&self, ticket: u64) -> Option<String> {
        self.tickets.iter().find(|(_, t)| t.id() == ticket).map(|(id, _)| id.clone())
    }

    /// Admit and restart every shed node that has been down long enough.
    fn restore(&mut self) -> Vec<String> {
        let due: Vec<String> = self
            .shed
            .iter()
            .filter(|(_, (_, at))| at.elapsed() >= RESTORE_AFTER)
            .map(|(id, _)| id.clone())
            .collect();
        let mut back = Vec::new();
        let cat = crate::catalogue::global();
        for id in due {
            let Some(node) = self.plan.node(&id).cloned() else {
                self.shed.remove(&id);
                continue;
            };
            let Ok(ticket) = admit(self.station.governor(), &node, &self.rungs, &HashMap::new()) else { continue };
            let preset = ticket.as_ref().and_then(|t| t.preset().map(str::to_string));
            if self.graph.resume(&node, &self.plan, &cat, preset.as_deref()).is_err() {
                continue;
            }
            if let Some(t) = ticket {
                self.tickets.insert(id.clone(), t);
            }
            self.shed.remove(&id);
            info!(node = %id, "rendition brought back");
            back.push(super::admit::describe(&node));
        }
        back
    }

    /// Why an output is not getting its rendition now, if it is not.
    pub fn shed_reason(&self, output: &str) -> Option<String> {
        let mine = self.requests_of(output);
        self.shed.iter().find_map(|(id, (why, _))| {
            let node = self.plan.node(id)?;
            node.serves.iter().any(|r| mine.contains(r)).then(|| why.clone())
        })
    }
}
