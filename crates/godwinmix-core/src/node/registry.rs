//! What the core knows about its nodes.
//!
//! One record per node: the ones listed in the config and never seen yet, the
//! ones connected now, and the ones that were connected and went. A record
//! survives the connection, because "studio-b, offline for 40 seconds" is the
//! thing an operator needs to see, and because the reconciler's desired state
//! has to keep naming a node that is not answering.

use super::bridge::Peer;
use super::wire::{
    Heartbeat, Hello, InstanceReport, NodeInstance, NodePlugin, NodeView, HEARTBEAT_TOLERANCE_MS,
};
use parking_lot::RwLock;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

/// One node, connected or not.
pub struct NodeRecord {
    pub name: String,
    /// What the config said, when the config said anything. A discovered node
    /// fills this in from the socket it arrived on.
    pub address: Option<String>,
    /// The SPIFFE identity in the certificate it presented.
    pub identity: Option<String>,
    pub version: Option<String>,
    pub platform: Option<String>,
    /// The live bridge. `None` while the node is away.
    pub link: Option<Arc<Peer>>,
    pub last_beat: Option<Instant>,
    pub clock_offset_ms: f64,
    pub clock_jitter_ms: f64,
    pub clock_synced: bool,
    pub plugins: Vec<NodePlugin>,
    pub instances: Vec<InstanceReport>,
    /// Where this node sends media from.
    pub media_host: String,
    /// True for a node that was written into the config and has never dialled
    /// in. The difference matters: an expected node that never arrives is a
    /// configuration mistake, and one that arrived and left is a network fault.
    pub expected_only: bool,
}

impl NodeRecord {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            address: None,
            identity: None,
            version: None,
            platform: None,
            link: None,
            last_beat: None,
            clock_offset_ms: 0.0,
            clock_jitter_ms: 0.0,
            clock_synced: false,
            plugins: Vec::new(),
            instances: Vec::new(),
            media_host: String::new(),
            expected_only: true,
        }
    }

    /// Online means beating, not merely connected.
    ///
    /// Three missed beats is the tolerance 04 section 5 sets. A socket that is
    /// still open on a machine that has stopped answering is exactly the case
    /// this has to catch, so the heartbeat is the test and the socket is not.
    pub fn online(&self) -> bool {
        !self.expected_only && self.heartbeat_age_ms() < HEARTBEAT_TOLERANCE_MS
    }

    pub fn heartbeat_age_ms(&self) -> u64 {
        match self.last_beat {
            Some(at) => at.elapsed().as_millis() as u64,
            None => u64::MAX / 2,
        }
    }

    pub fn state(&self) -> &'static str {
        if self.online() {
            "online"
        } else if self.expected_only && self.link.is_none() {
            "expected"
        } else {
            "offline"
        }
    }

    pub fn view(&self) -> NodeView {
        NodeView {
            name: self.name.clone(),
            state: self.state().into(),
            address: self.address.clone(),
            identity: self.identity.clone(),
            version: self.version.clone(),
            platform: self.platform.clone(),
            heartbeat_age_ms: self.heartbeat_age_ms().min(u64::MAX / 2),
            clock_offset_ms: self.clock_offset_ms,
            clock_jitter_ms: self.clock_jitter_ms,
            clock_synced: self.clock_synced,
            provides: self
                .plugins
                .iter()
                .flat_map(|p| p.provides.iter().cloned())
                .collect(),
            plugins: self.plugins.clone(),
            instances: self
                .instances
                .iter()
                .map(|i| NodeInstance {
                    instance: i.instance.clone(),
                    state: i.state.clone(),
                    detail: i.detail.clone(),
                    latency_ms: i.latency_ms,
                })
                .collect(),
        }
    }
}

/// Every node the core knows about.
#[derive(Default)]
pub struct Nodes {
    inner: RwLock<BTreeMap<String, NodeRecord>>,
}

impl Nodes {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Write down a node the config expects, without a connection.
    pub fn expect(&self, name: &str, address: Option<String>) {
        let mut inner = self.inner.write();
        let record = inner.entry(name.to_string()).or_insert_with(|| NodeRecord::new(name));
        if address.is_some() {
            record.address = address;
        }
    }

    /// A node has connected. Replaces any previous link.
    pub fn joined(&self, hello: &Hello, identity: Option<String>, from: String, link: Arc<Peer>) {
        self.record(hello, identity, from, Some(link));
    }

    /// The same, with no socket behind it. Tests build a registry this way so
    /// the reconciler can be exercised without a network.
    #[cfg(test)]
    pub fn joined_without_link(&self, hello: &Hello, from: String) {
        self.record(hello, None, from, None);
    }

    fn record(
        &self,
        hello: &Hello,
        identity: Option<String>,
        from: String,
        link: Option<Arc<Peer>>,
    ) {
        let mut inner = self.inner.write();
        let record = inner.entry(hello.name.clone()).or_insert_with(|| NodeRecord::new(&hello.name));
        let old = std::mem::replace(&mut record.link, link);
        if let Some(old) = old {
            old.close("the node connected again, so the old bridge is stale");
        }
        record.identity = identity;
        record.version = Some(hello.version.clone());
        record.platform = Some(hello.platform.clone());
        record.address = Some(from.clone());
        record.media_host = if hello.media_host.is_empty() {
            from.split(':').next().unwrap_or(&from).to_string()
        } else {
            hello.media_host.clone()
        };
        record.plugins = hello
            .plugins
            .iter()
            .map(|m| NodePlugin {
                name: m.plugin.name.clone(),
                version: m.plugin.version.clone(),
                provides: m
                    .provides
                    .iter()
                    .map(|p| format!("{}/{}", m.plugin.name, p.id))
                    .collect(),
            })
            .collect();
        record.expected_only = false;
        record.last_beat = Some(Instant::now());
        record.clock_synced = false;
        record.instances.clear();
    }

    /// A node's bridge has gone.
    pub fn left(&self, name: &str, why: &str) {
        let mut inner = self.inner.write();
        if let Some(record) = inner.get_mut(name) {
            record.link = None;
            record.last_beat = None;
            record.clock_synced = false;
            // The instance list is kept: it is the last thing the node said,
            // and the reconciler wants to know what it was carrying.
            tracing::warn!(node = %name, why, "the bridge to a node is down");
        }
    }

    /// One beat arrived.
    pub fn beat(&self, name: &str, beat: &Heartbeat) {
        let mut inner = self.inner.write();
        let Some(record) = inner.get_mut(name) else { return };
        record.last_beat = Some(Instant::now());
        record.clock_offset_ms = beat.clock_offset_ms;
        record.clock_jitter_ms = beat.clock_jitter_ms;
        record.clock_synced = beat.clock_synced;
        record.instances = beat.instances.clone();
        crate::observe::metrics::gauge("gmx_node_clock_offset_ms", &[("node", name)])
            .set(beat.clock_offset_ms);
        crate::observe::metrics::gauge("gmx_node_heartbeat_age_ms", &[("node", name)]).set(0.0);
    }

    /// Refresh the heartbeat age gauges. Called once a reconciler tick, so a
    /// node that went silent shows a climbing age rather than a stale zero.
    pub fn sample_metrics(&self) {
        for record in self.inner.read().values() {
            let age = record.heartbeat_age_ms().min(600_000) as f64;
            crate::observe::metrics::gauge("gmx_node_heartbeat_age_ms", &[("node", &record.name)])
                .set(age);
        }
    }

    /// The live bridge to one node, if it has one.
    pub fn link(&self, name: &str) -> Option<Arc<Peer>> {
        self.inner.read().get(name).and_then(|r| r.link.clone()).filter(|l| !l.is_closed())
    }

    /// Is this the bridge the node is on right now?
    ///
    /// A node that reconnects gets a new connection, and the one it replaced
    /// has a task still winding down behind it. That task must not tear down
    /// the node the new connection has just set up, so it asks this first.
    pub fn holds(&self, name: &str, peer: &Arc<Peer>) -> bool {
        self.inner
            .read()
            .get(name)
            .and_then(|r| r.link.as_ref())
            .is_some_and(|live| Arc::ptr_eq(live, peer))
    }

    /// How long since this node last beat. `None` if there is no such node.
    pub fn heartbeat_age_ms(&self, name: &str) -> Option<u64> {
        self.inner.read().get(name).map(NodeRecord::heartbeat_age_ms)
    }

    /// What this node's bridge is doing, in words. For a log line and for the
    /// failure message of a test that expected it to have gone.
    pub fn link_state(&self, name: &str) -> String {
        match self.inner.read().get(name) {
            None => "no record".to_string(),
            Some(record) => match &record.link {
                None => "no link".to_string(),
                Some(link) if link.is_closed() => {
                    format!("closed ({})", link.reason().unwrap_or_else(|| "no reason".into()))
                }
                Some(_) => "open".to_string(),
            },
        }
    }

    /// Where a node sends media from, for building the core's receive side.
    pub fn media_host(&self, name: &str) -> Option<String> {
        self.inner.read().get(name).map(|r| r.media_host.clone()).filter(|h| !h.is_empty())
    }

    pub fn view(&self, name: &str) -> Option<NodeView> {
        self.inner.read().get(name).map(NodeRecord::view)
    }

    pub fn views(&self) -> Vec<NodeView> {
        self.inner.read().values().map(NodeRecord::view).collect()
    }

    pub fn names(&self) -> Vec<String> {
        self.inner.read().keys().cloned().collect()
    }

    pub fn is_online(&self, name: &str) -> bool {
        self.inner.read().get(name).is_some_and(NodeRecord::online)
    }

    /// The nodes that were online at the last tick and are not now. The
    /// reconciler turns each into a freeze frame and an alert.
    pub fn lost(&self) -> Vec<String> {
        self.inner
            .read()
            .values()
            .filter(|r| !r.online() && !r.expected_only)
            .map(|r| r.name.clone())
            .collect()
    }

    /// Forget a node entirely. `node.remove`.
    pub fn remove(&self, name: &str) -> bool {
        let gone = self.inner.write().remove(name);
        match gone {
            Some(record) => {
                if let Some(link) = record.link {
                    link.close("this node was removed from the core");
                }
                true
            }
            None => false,
        }
    }

    /// Every provide id reachable on any online node, with the node that has
    /// it. This is what puts a remote plugin in the picker.
    pub fn provides(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for record in self.inner.read().values() {
            if !record.online() {
                continue;
            }
            for plugin in &record.plugins {
                for provide in &plugin.provides {
                    out.push((provide.clone(), record.name.clone()));
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_expected_node_is_not_the_same_as_an_absent_one() {
        let nodes = Nodes::new();
        nodes.expect("studio-b", Some("10.0.0.21:8443".into()));
        let view = nodes.view("studio-b").unwrap();
        assert_eq!(view.state, "expected");
        assert_eq!(view.address.as_deref(), Some("10.0.0.21:8443"));
        assert!(nodes.view("studio-c").is_none());
    }

    #[test]
    fn removing_a_node_forgets_it() {
        let nodes = Nodes::new();
        nodes.expect("studio-b", None);
        assert!(nodes.remove("studio-b"));
        assert!(!nodes.remove("studio-b"));
        assert!(nodes.views().is_empty());
    }
}
