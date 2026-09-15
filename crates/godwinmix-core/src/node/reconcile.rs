//! Desired state against what each node reports.
//!
//! The core holds what the operator asked for: the config plus every runtime
//! change. Each node reports what it actually has. This is the loop that
//! closes the gap, in the shape Sofie and Kubernetes use, with the restart
//! intensity LiveboxMix already applied to superimposed sources: three free
//! restarts, then 30 seconds doubling to 300, cleared on the first frame or on
//! removal.
//!
//! ```text
//!  desired (store)                        actual (reported)
//!  sources:                               node cam-room:
//!    cam1  ndi/source  node:cam-room        cam1  running  latency 82 ms
//!    cam2  ndi/source  node:cam-room        cam2  failed   "sender not found"
//!    score browser     node:graphics-pc   node graphics-pc:
//!                                           (no heartbeat for 4 s)
//!
//!  decisions this tick:
//!    cam2   restart on cam-room, attempt 2 of 3 free, then backoff 30 s
//!    score  mark failed, hold freeze frame, alert "graphics-pc unreachable"
//!    cam1   nothing
//! ```
//!
//! It never blocks the programme. The reconciler produces a list of actions
//! and somebody else carries them out; nothing here waits on a network, a
//! plugin or a pipeline. That is why a node on the other side of a cut cable
//! costs one tick and not one frame.

use super::registry::Nodes;
use super::wire::BridgeTransport;
use super::Place;
use godwinmix_host::lifecycle::Backoff;
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Instant;

/// One thing the operator asked for, at whatever placement they asked for it.
#[derive(Debug, Clone)]
pub struct Desired {
    pub instance: String,
    /// `<plugin>/<provide>`.
    pub type_id: String,
    pub place: Place,
    pub params: Value,
    pub transport: BridgeTransport,
    /// The latency budget, when the operator set one. `None` takes the
    /// transport's default.
    pub latency_ms: Option<u32>,
}

/// What the reconciler decided this tick. Nothing here has happened yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Start this instance on this node, because the core wants it and the
    /// node does not have it.
    Start { instance: String, node: String },
    /// Stop it, because the node has it and the core no longer wants it.
    Stop { instance: String, node: String, reason: String },
    /// It failed on the node. This is the nth attempt.
    Restart { instance: String, node: String, attempt: u32 },
    /// It failed and the backoff has not run out. Nothing until then.
    Wait { instance: String, node: String, secs: u64 },
    /// Its node is unreachable. Hold the freeze frame, then the slate, and
    /// raise an alert.
    Unreachable { instance: String, node: String, silent_ms: u64 },
    /// A node that was unreachable answered again. The alert clears.
    Back { node: String },
}

impl Action {
    /// A line for the log, in the style of the supervisor's own decisions:
    /// what was decided and the number that decided it.
    pub fn why(&self) -> String {
        match self {
            Action::Start { instance, node } => {
                format!("start {instance} on {node}: the core wants it and the node has not got it")
            }
            Action::Stop { instance, node, reason } => {
                format!("stop {instance} on {node}: {reason}")
            }
            Action::Restart { instance, node, attempt } => {
                format!("restart {instance} on {node}: it failed, attempt {attempt}")
            }
            Action::Wait { instance, node, secs } => {
                format!("hold {instance} on {node}: it keeps failing, {secs} s of backoff left")
            }
            Action::Unreachable { instance, node, silent_ms } => format!(
                "{instance} is on {node} and {node} has said nothing for {silent_ms} ms: freeze \
                 frame, then the slate"
            ),
            Action::Back { node } => format!("{node} is answering again"),
        }
    }
}

/// The loop that closes the gap.
pub struct Reconciler {
    nodes: Arc<Nodes>,
    desired: Mutex<BTreeMap<String, Desired>>,
    backoff: Mutex<BTreeMap<String, (Backoff, Option<Instant>)>>,
    /// Nodes an alert has already been raised about, so the alert is raised
    /// once per outage and not once a tick.
    alerted: Mutex<BTreeSet<String>>,
}

impl Reconciler {
    pub fn new(nodes: Arc<Nodes>) -> Arc<Self> {
        Arc::new(Self {
            nodes,
            desired: Mutex::new(BTreeMap::new()),
            backoff: Mutex::new(BTreeMap::new()),
            alerted: Mutex::new(BTreeSet::new()),
        })
    }

    /// Write down what the operator wants. Replaces any earlier answer for the
    /// same instance, which is what makes `source.set {place}` a move rather
    /// than a second source.
    pub fn want(&self, desired: Desired) {
        self.backoff.lock().remove(&desired.instance);
        self.desired.lock().insert(desired.instance.clone(), desired);
    }

    /// Stop wanting it.
    pub fn forget(&self, instance: &str) -> Option<Desired> {
        self.backoff.lock().remove(instance);
        self.desired.lock().remove(instance)
    }

    pub fn wanted(&self, instance: &str) -> Option<Desired> {
        self.desired.lock().get(instance).cloned()
    }

    /// Everything the core wants on one node.
    pub fn on_node(&self, node: &str) -> Vec<Desired> {
        self.desired
            .lock()
            .values()
            .filter(|d| d.place.node() == Some(node))
            .cloned()
            .collect()
    }

    /// One pass. Returns what to do, in the order to do it.
    pub fn tick(&self) -> Vec<Action> {
        self.nodes.sample_metrics();
        let mut actions = Vec::new();
        let desired = self.desired.lock().clone();
        // A node that went quiet and has come back clears its alert. Done
        // first, so the instances on it are judged as running rather than as
        // unreachable in the same tick.
        actions.extend(self.settle_alerts());
        for want in desired.values() {
            let Some(node) = want.place.node() else { continue };
            actions.extend(self.one(want, node));
        }
        actions.extend(self.strays(&desired));
        actions
    }

    /// The decision for one wanted instance.
    fn one(&self, want: &Desired, node: &str) -> Vec<Action> {
        let Some(view) = self.nodes.view(node) else {
            // A node named in a source's `place` that the core has never heard
            // of. Reported as unreachable rather than dropped, because the
            // operator wrote it down and needs to see that it is wrong.
            return vec![Action::Unreachable {
                instance: want.instance.clone(),
                node: node.to_string(),
                silent_ms: 0,
            }];
        };
        if view.state != "online" {
            let first = self.alerted.lock().insert(node.to_string());
            if first {
                tracing::warn!(node, state = %view.state, "a node carrying sources is unreachable");
            }
            return vec![Action::Unreachable {
                instance: want.instance.clone(),
                node: node.to_string(),
                silent_ms: view.heartbeat_age_ms,
            }];
        }
        let found = view.instances.iter().find(|i| i.instance == want.instance);
        match found {
            None => vec![Action::Start {
                instance: want.instance.clone(),
                node: node.to_string(),
            }],
            Some(instance) if instance.state == "failed" => {
                self.after_failure(&want.instance, node)
            }
            Some(_) => {
                // It is there and it is not failed. Clear the backoff: three
                // free restarts are three per outage, not three for ever.
                self.backoff.lock().remove(&want.instance);
                Vec::new()
            }
        }
    }

    /// Restart, or wait, depending on how often this one has failed.
    fn after_failure(&self, instance: &str, node: &str) -> Vec<Action> {
        let mut book = self.backoff.lock();
        let entry = book.entry(instance.to_string()).or_insert((Backoff::default(), None));
        if let Some(not_before) = entry.1 {
            if Instant::now() < not_before {
                return vec![Action::Wait {
                    instance: instance.to_string(),
                    node: node.to_string(),
                    secs: (not_before - Instant::now()).as_secs(),
                }];
            }
        }
        let wait = entry.0.next_wait();
        let attempt = entry.0.attempts();
        if wait.is_zero() {
            entry.1 = None;
            vec![Action::Restart {
                instance: instance.to_string(),
                node: node.to_string(),
                attempt,
            }]
        } else {
            entry.1 = Some(Instant::now() + wait);
            vec![Action::Wait {
                instance: instance.to_string(),
                node: node.to_string(),
                secs: wait.as_secs(),
            }]
        }
    }

    /// Anything a node is running that the core no longer wants there.
    fn strays(&self, desired: &BTreeMap<String, Desired>) -> Vec<Action> {
        let mut actions = Vec::new();
        for view in self.nodes.views() {
            if view.state != "online" {
                continue;
            }
            for instance in &view.instances {
                let reason = match desired.get(&instance.instance) {
                    None => "the core no longer has that source".to_string(),
                    Some(want) if want.place.node() != Some(view.name.as_str()) => {
                        format!("that source moved to {}", want.place)
                    }
                    Some(_) => continue,
                };
                actions.push(Action::Stop {
                    instance: instance.instance.clone(),
                    node: view.name.clone(),
                    reason,
                });
            }
        }
        actions
    }

    /// Nodes that were alerted about and are answering again.
    fn settle_alerts(&self) -> Vec<Action> {
        let back: Vec<String> = self
            .alerted
            .lock()
            .iter()
            .filter(|node| self.nodes.is_online(node))
            .cloned()
            .collect();
        let mut alerted = self.alerted.lock();
        for node in &back {
            alerted.remove(node);
            tracing::info!(node, "a node is answering again; its sources will be started back up");
        }
        back.into_iter().map(|node| Action::Back { node }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::wire::{Hello, InstanceReport};

    fn desired(instance: &str, node: &str) -> Desired {
        Desired {
            instance: instance.into(),
            type_id: "test/source".into(),
            place: Place::Node(node.into()),
            params: Value::Null,
            transport: BridgeTransport::Srt,
            latency_ms: None,
        }
    }

    /// A registry with one node that is online and reporting `instances`.
    fn online(name: &str, instances: Vec<InstanceReport>) -> Arc<Nodes> {
        let nodes = Nodes::new();
        let hello = Hello {
            name: name.into(),
            version: "0.2.0".into(),
            api: 1,
            platform: "linux-x86_64".into(),
            plugins: Vec::new(),
            schemas: Default::default(),
            media_host: "10.0.0.21".into(),
        };
        nodes.joined_without_link(&hello, "10.0.0.21:9000".into());
        nodes.beat(
            name,
            &super::super::wire::Heartbeat {
                ts_unix_ms: 0,
                clock_offset_ms: 0.2,
                clock_jitter_ms: 0.1,
                clock_synced: true,
                instances,
            },
        );
        nodes
    }

    #[test]
    fn a_source_the_node_has_not_got_is_started() {
        let nodes = online("cam-room", Vec::new());
        let r = Reconciler::new(nodes);
        r.want(desired("cam1", "cam-room"));
        let actions = r.tick();
        assert!(
            actions.contains(&Action::Start {
                instance: "cam1".into(),
                node: "cam-room".into()
            }),
            "got {actions:?}"
        );
    }

    #[test]
    fn a_source_on_a_node_the_core_has_never_heard_of_is_unreachable() {
        let r = Reconciler::new(Nodes::new());
        r.want(desired("cam1", "nowhere"));
        let actions = r.tick();
        assert!(
            matches!(actions.as_slice(), [Action::Unreachable { node, .. }] if node == "nowhere"),
            "got {actions:?}"
        );
        assert!(actions[0].why().contains("freeze frame"));
    }

    #[test]
    fn forgetting_a_source_stops_wanting_it() {
        let r = Reconciler::new(Nodes::new());
        r.want(desired("cam1", "cam-room"));
        assert!(r.wanted("cam1").is_some());
        assert!(r.forget("cam1").is_some());
        assert!(r.wanted("cam1").is_none());
        assert!(r.tick().is_empty());
    }

    #[test]
    fn every_action_says_why() {
        let all = [
            Action::Start { instance: "a".into(), node: "n".into() },
            Action::Stop { instance: "a".into(), node: "n".into(), reason: "gone".into() },
            Action::Restart { instance: "a".into(), node: "n".into(), attempt: 2 },
            Action::Wait { instance: "a".into(), node: "n".into(), secs: 30 },
            Action::Unreachable { instance: "a".into(), node: "n".into(), silent_ms: 4000 },
            Action::Back { node: "n".into() },
        ];
        for action in all {
            assert!(action.why().len() > 10, "a decision must be legible: {action:?}");
        }
    }
}
