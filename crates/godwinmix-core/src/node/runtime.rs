//! The core's node machinery, in one place, started once.
//!
//! The registry, the certificate authority, the enrolment tokens, the media
//! port pool, the clock provider and the reconciler. A process global, like the
//! plugin loader's registry and for the same reason: a source is built from a
//! static function pointer in the provide table, which has nowhere to carry a
//! handle. Nothing here exists until `start` is called, and `start` is only
//! called when the config asks for nodes, so a core nobody has enrolled a node
//! with opens no port, makes no certificate and starts no clock.

use super::bridge::Peer;
use super::ca::NodeCa;
use super::clock;
use super::enrol::Tickets;
use super::media::PortPool;
use super::reconcile::Reconciler;
use super::registry::Nodes;
use super::server::{NodeServer, Watcher};
use super::wire::{BridgeTransport, ClockOffer, MediaPlan, CALL_TIMEOUT_MS};
use crate::plugin::host::LinkSource;
use anyhow::{Context, Result};
use parking_lot::Mutex;
use serde_json::Value;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// How often the reconciler compares desired state with what nodes report.
///
/// Four times a second: fast enough that a node coming back is noticed inside
/// the freeze frame, slow enough that it costs nothing. The programme never
/// waits on it, because it produces a list and somebody else acts on it.
pub const TICK: Duration = Duration::from_millis(250);

pub struct Runtime {
    pub nodes: Arc<Nodes>,
    pub tickets: Arc<Tickets>,
    pub ca: Arc<NodeCa>,
    pub reconciler: Arc<Reconciler>,
    pub ports: PortPool,
    /// The tokio runtime the bridge lives on. Every `Source` method is
    /// synchronous and the bridge is not; this is where the two meet.
    handle: tokio::runtime::Handle,
    /// The address a node should send media to. Learned from the socket a node
    /// arrived on, because the core does not know which of its addresses is
    /// the one the node can reach.
    core_host: Mutex<String>,
    clock_port: i32,
    clock_kind: String,
    /// Held for the life of the process. Dropping it takes the clock off the
    /// network and every node loses sync.
    _clock: Option<clock::Provider>,
    _advert: Option<super::discovery::Advertisement>,
}

static RUNTIME: OnceLock<Arc<Runtime>> = OnceLock::new();

/// The node machinery, if this core has any.
pub fn get() -> Option<Arc<Runtime>> {
    RUNTIME.get().cloned()
}

/// What `start` needs from the config and the mixer.
pub struct Options {
    /// `<runtime>/nodes`, where the CA and the token book live.
    pub dir: std::path::PathBuf,
    pub bind: String,
    pub server_names: Vec<String>,
    pub clock_port: i32,
    pub clock_kind: String,
    pub advertise: bool,
    /// The programme clock, which every node slaves to.
    pub clock: gstreamer::Clock,
    pub canvas: godwinmix_protocol::plugin::wire::Canvas,
    /// The nodes the config expects, as name and address.
    pub expected: Vec<(String, Option<String>)>,
    /// Where a node's events go: the mixer's alert path, the event bus, the
    /// log. Called from the bridge's task, so it must not block.
    pub watch: Watcher,
}

/// Bring the node machinery up. Returns the address the bridge is listening on.
pub async fn start(options: Options) -> Result<SocketAddr> {
    let ca = Arc::new(NodeCa::open_or_create(&options.dir.join("ca"))?);
    let tickets = Arc::new(Tickets::open(&options.dir.join("enrolment.json")));
    let nodes = Nodes::new();
    for (name, address) in &options.expected {
        nodes.expect(name, address.clone());
    }
    let reconciler = Reconciler::new(nodes.clone());
    let provider = clock::Provider::publish(&options.clock, options.clock_port)
        .context("put the programme clock on the network for nodes")?;
    let clock_port = provider.port();
    let bind: SocketAddr = options
        .bind
        .parse()
        .with_context(|| format!("`{}` is not an address to listen on", options.bind))?;

    let advert = options
        .advertise
        .then(|| {
            super::discovery::Advertisement::start("core", "core", bind.port(), super::wire::BRIDGE_API)
        })
        .transpose()
        .unwrap_or_else(|e| {
            tracing::warn!(error = %format!("{e:#}"), "not advertising over mDNS; nodes must be listed in [nodes]");
            None
        });

    let runtime = Arc::new(Runtime {
        nodes: nodes.clone(),
        tickets: tickets.clone(),
        ca: ca.clone(),
        reconciler: reconciler.clone(),
        ports: PortPool::new(),
        handle: tokio::runtime::Handle::current(),
        core_host: Mutex::new(String::new()),
        clock_port,
        clock_kind: options.clock_kind.clone(),
        _clock: Some(provider),
        _advert: advert,
    });
    let server = NodeServer::new(
        ca,
        tickets,
        nodes,
        options.canvas,
        ClockOffer {
            // Empty: the node fills in the address it dialled, which is the
            // one it can reach. The core cannot know which of its addresses
            // that is.
            host: String::new(),
            port: clock_port,
            kind: options.clock_kind,
        },
        options.watch,
    );
    let bound = server.serve(bind, &options.server_names).await?;
    let _ = RUNTIME.set(runtime);
    Ok(bound)
}

impl Runtime {
    /// The address nodes should send RTP to, learned from the first node that
    /// connected. Falls back to the loopback, which is right for a node in a
    /// child process and is what the smoke test uses.
    pub fn core_host(&self) -> String {
        let learned = self.core_host.lock().clone();
        if learned.is_empty() {
            "127.0.0.1".into()
        } else {
            learned
        }
    }

    pub fn set_core_host(&self, host: &str) {
        *self.core_host.lock() = host.to_string();
    }

    pub fn clock_port(&self) -> i32 {
        self.clock_port
    }

    pub fn clock_kind(&self) -> &str {
        &self.clock_kind
    }

    /// Settle how one source's media will travel between a node and here.
    pub fn plan(
        &self,
        node: &str,
        transport: BridgeTransport,
        latency_ms: Option<u32>,
    ) -> Result<MediaPlan> {
        let media_host = self.nodes.media_host(node).with_context(|| {
            format!(
                "`{node}` has not connected, so the core does not know where it is. `node.list` \
                 says which nodes are online"
            )
        })?;
        super::media::plan(transport, &media_host, &self.core_host(), &self.ports, latency_ms)
    }

    /// One reconciler pass, with the actions carried out.
    ///
    /// `act` is given each decision in order. It runs on the caller's thread,
    /// which is the tick task and never a streaming thread.
    pub fn tick(&self, act: impl Fn(&super::reconcile::Action)) {
        for action in self.reconciler.tick() {
            tracing::debug!(why = %action.why(), "node reconciler");
            act(&action);
        }
    }
}

impl LinkSource for Runtime {
    fn link(&self, node: &str) -> Option<Arc<Peer>> {
        self.nodes.link(node)
    }

    /// One call on the bridge, from a thread that is not async.
    ///
    /// The future is spawned onto the runtime and waited for on a channel,
    /// rather than `block_on`, because the caller may be a tokio worker and
    /// blocking one of those inside another is how a runtime deadlocks. The
    /// wait is bounded: a node that has gone fails the call rather than
    /// holding a mixer command open.
    fn call(&self, node: &str, method: &str, params: Value) -> Result<Value> {
        let peer = self.nodes.link(node).with_context(|| {
            format!(
                "`{method}` cannot go anywhere: the node `{node}` is not connected. It comes back \
                 on its own; watch `node.get`"
            )
        })?;
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let method = method.to_string();
        self.handle.spawn(async move {
            let _ = tx.send(peer.call(&method, params).await);
        });
        rx.recv_timeout(Duration::from_millis(CALL_TIMEOUT_MS + 500))
            .context("the node bridge did not answer in time")?
    }
}

/// Install a runtime for a test or an embedder. Only the first one counts.
pub fn set_for_test(runtime: Arc<Runtime>) -> bool {
    RUNTIME.set(runtime).is_ok()
}

/// Build a runtime with nothing behind it, for tests that need the registry
/// and the reconciler but no sockets.
pub fn detached(dir: &Path) -> Result<Arc<Runtime>> {
    let nodes = Nodes::new();
    Ok(Arc::new(Runtime {
        reconciler: Reconciler::new(nodes.clone()),
        nodes,
        tickets: Arc::new(Tickets::detached()),
        ca: Arc::new(NodeCa::open_or_create(dir)?),
        ports: PortPool::new(),
        handle: tokio::runtime::Handle::current(),
        core_host: Mutex::new("127.0.0.1".into()),
        clock_port: 0,
        clock_kind: "net".into(),
        _clock: None,
        _advert: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn a_call_to_a_node_that_is_not_there_fails_with_the_node_named() {
        let dir = std::env::temp_dir().join(format!("gmx-rt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let runtime = detached(&dir).unwrap();
        let e = runtime.call("studio-b", "health", Value::Null).unwrap_err().to_string();
        assert!(e.contains("studio-b"), "{e}");
        assert!(e.contains("node.get"), "the refusal must say where to look: {e}");
        assert!(runtime.link("studio-b").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_plan_needs_the_node_to_have_connected() {
        let dir = std::env::temp_dir().join(format!("gmx-rt2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let runtime = detached(&dir).unwrap();
        let e = runtime
            .plan("studio-b", BridgeTransport::Srt, None)
            .unwrap_err()
            .to_string();
        assert!(e.contains("node.list"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
