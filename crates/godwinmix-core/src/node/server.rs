//! The core's side of the bridge: one TLS listener, one WebSocket per node.
//!
//! Deliberately not the axum control server. A node is not a browser: it
//! presents a client certificate, it is called as often as it calls, and it
//! must keep working when the control port is behind a reverse proxy that
//! knows nothing about client certificates. One listener, one accept loop, and
//! the same JSON-RPC everything else speaks.
//!
//! Two kinds of connection arrive here:
//!
//! * A node with no certificate. It may say exactly one thing, `node.enrol`,
//!   and it gets a certificate or a refusal and then the socket closes. This
//!   is the only moment in the life of a node when a secret crosses in the
//!   clear, and the secret is good once.
//! * A node with a certificate this core signed. Its name comes out of the
//!   SPIFFE identity in the certificate, not out of anything it says, so a
//!   node cannot claim to be another one.

use super::bridge::{Answer, Handler, Peer};
use super::ca::{self, NodeCa};
use super::enrol::Tickets;
use super::registry::Nodes;
use super::wire::{ClockOffer, Heartbeat, Hello, Welcome, BRIDGE_API};
use anyhow::{Context, Result};
use godwinmix_protocol::plugin::wire::Canvas;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

/// The port a core listens for nodes on unless the config says otherwise.
pub const DEFAULT_PORT: u16 = 8443;

/// Something that wants to know when a node does something.
///
/// A callback rather than an event bus, because the engine's event bus lives
/// above this module and a node that joins has to reach the mixer's alert
/// path, the loader's remote plugin table and the reconciler, which are three
/// different owners.
pub type Watcher = Arc<dyn Fn(NodeEvent) + Send + Sync>;

/// Where one connection's peer waits between the WebSocket handshake and the
/// hello that identifies it.
type Slot = Arc<parking_lot::Mutex<Option<Arc<Peer>>>>;

#[derive(Debug, Clone)]
pub enum NodeEvent {
    /// A node connected and said what it has.
    Joined { node: String, hello: Box<Hello> },
    /// A node's bridge went, for whatever reason.
    Left { node: String, why: String },
    /// One instance on a node changed state.
    Instance { node: String, instance: String, state: String, detail: Option<String> },
    /// A plugin on a node raised an event.
    Event { node: String, instance: Option<String>, name: String, params: Value },
    /// A line from a plugin's log on a node.
    Log { node: String, instance: Option<String>, level: String, message: String },
}

/// Everything the listener needs to answer a node.
pub struct NodeServer {
    ca: Arc<NodeCa>,
    tickets: Arc<Tickets>,
    nodes: Arc<Nodes>,
    canvas: Canvas,
    clock: ClockOffer,
    watch: Watcher,
}

impl NodeServer {
    pub fn new(
        ca: Arc<NodeCa>,
        tickets: Arc<Tickets>,
        nodes: Arc<Nodes>,
        canvas: Canvas,
        clock: ClockOffer,
        watch: Watcher,
    ) -> Arc<Self> {
        Arc::new(Self { ca, tickets, nodes, canvas, clock, watch })
    }

    /// Bind, and run the accept loop on a task of its own.
    ///
    /// Returns the address it actually bound, so a test can ask for port 0 and
    /// find out where it landed.
    pub async fn serve(self: &Arc<Self>, bind: SocketAddr, names: &[String]) -> Result<SocketAddr> {
        let tls = TlsAcceptor::from(
            self.ca.server_config(names).context("build the node listener's TLS configuration")?,
        );
        let listener = TcpListener::bind(bind)
            .await
            .with_context(|| format!("bind the node listener on {bind}"))?;
        let bound = listener.local_addr()?;
        tracing::info!(address = %bound, "the node bridge is listening");
        let me = self.clone();
        tokio::spawn(async move {
            loop {
                let (socket, from) = match listener.accept().await {
                    Ok(pair) => pair,
                    Err(e) => {
                        tracing::warn!(?e, "the node listener could not accept");
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        continue;
                    }
                };
                let tls = tls.clone();
                let me = me.clone();
                tokio::spawn(async move {
                    if let Err(e) = me.one(socket, from, tls).await {
                        tracing::info!(%from, error = %format!("{e:#}"), "a node connection ended");
                    }
                });
            }
        });
        Ok(bound)
    }

    /// One connection, from the TCP accept to the socket closing.
    async fn one(
        self: Arc<Self>,
        socket: tokio::net::TcpStream,
        from: SocketAddr,
        tls: TlsAcceptor,
    ) -> Result<()> {
        // Nagle off: every frame here is a command or a heartbeat and none of
        // them is worth a 40 ms delay waiting for a second one.
        let _ = socket.set_nodelay(true);
        let stream = tls.accept(socket).await.context("the TLS handshake failed")?;
        let identity = {
            let (_, session) = stream.get_ref();
            session
                .peer_certificates()
                .and_then(|chain| chain.first())
                .and_then(ca::identity_in)
        };
        let ws = tokio_tungstenite::accept_async(stream)
            .await
            .context("the WebSocket handshake failed")?;

        match identity {
            None => self.enrol_only(ws, from).await,
            Some(identity) => {
                let name = ca::name_in_spiffe(&identity)
                    .context("a certificate this core signed with no node name in it")?
                    .to_string();
                self.bridge(ws, from, name, identity).await
            }
        }
    }

    /// A connection with no certificate. One method, one answer, then gone.
    async fn enrol_only<S>(self: Arc<Self>, ws: S, from: SocketAddr) -> Result<()>
    where
        S: futures_util::Stream<Item = Result<tokio_tungstenite::tungstenite::Message, tokio_tungstenite::tungstenite::Error>>
            + futures_util::Sink<tokio_tungstenite::tungstenite::Message>
            + Send
            + 'static,
        <S as futures_util::Sink<tokio_tungstenite::tungstenite::Message>>::Error: std::fmt::Display,
    {
        let tickets = self.tickets.clone();
        let ca = self.ca.clone();
        let nodes = self.nodes.clone();
        let handler: Handler = Arc::new(move |method: String, params: Value| {
            let tickets = tickets.clone();
            let ca = ca.clone();
            let nodes = nodes.clone();
            let boxed: Answer = Box::pin(async move {
                if method != "node.enrol" {
                    return Err(super::wire::FrameError {
                        code: -32002,
                        message: format!(
                            "this connection presented no client certificate, so the only thing \
                             it may ask for is `node.enrol`, and it asked for `{method}`"
                        ),
                        data: None,
                    });
                }
                let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
                let token = params.get("token").and_then(Value::as_str).unwrap_or_default();
                if name.is_empty() {
                    return Err(super::wire::FrameError {
                        code: -32602,
                        message: "an enrolment needs a node name. Start the node with --name \
                                  <name>, matching the name the token was minted for"
                            .into(),
                        data: None,
                    });
                }
                match tickets.redeem(name, token) {
                    Ok(_) => {}
                    Err(refusal) => {
                        tracing::warn!(node = name, %from, refusal = %refusal, "an enrolment was refused");
                        return Err(super::wire::FrameError {
                            code: -32002,
                            message: refusal.to_string(),
                            data: Some(json!({ "retryable": false })),
                        });
                    }
                }
                let issued = ca.issue_node(name).map_err(|e| super::wire::FrameError {
                    code: -32603,
                    message: format!("the core could not sign a certificate: {e:#}"),
                    data: None,
                })?;
                nodes.expect(name, Some(from.to_string()));
                tracing::info!(node = name, %from, identity = %issued.identity, "a node enrolled");
                serde_json::to_value(&issued).map_err(|e| super::wire::FrameError {
                    code: -32603,
                    message: format!("encoding the certificate: {e}"),
                    data: None,
                })
            });
            boxed
        });
        let (peer, pump) = Peer::start(ws, handler);
        // An enrolment is one round trip. Thirty seconds is generous and it
        // stops an unauthenticated socket sitting there for the life of the
        // process.
        let why = tokio::time::timeout(std::time::Duration::from_secs(30), pump).await;
        peer.close("the enrolment is finished");
        match why {
            Ok(why) => tracing::debug!(%from, why, "an enrolment connection closed"),
            Err(_) => tracing::info!(%from, "an enrolment connection said nothing and timed out"),
        }
        Ok(())
    }

    /// A connection with a certificate: the real bridge.
    async fn bridge<S>(
        self: Arc<Self>,
        ws: S,
        from: SocketAddr,
        name: String,
        identity: String,
    ) -> Result<()>
    where
        S: futures_util::Stream<Item = Result<tokio_tungstenite::tungstenite::Message, tokio_tungstenite::tungstenite::Error>>
            + futures_util::Sink<tokio_tungstenite::tungstenite::Message>
            + Send
            + 'static,
        <S as futures_util::Sink<tokio_tungstenite::tungstenite::Message>>::Error: std::fmt::Display,
    {
        // The registry does not learn about this node until it says hello, so
        // the peer is parked in a slot that belongs to this connection alone.
        // One slot per server would be a race the first time two nodes
        // reconnected in the same instant.
        let slot: Slot = Arc::new(parking_lot::Mutex::new(None));
        let handler = self.clone().handler(name.clone(), from, identity, slot.clone());
        let (peer, pump) = Peer::start(ws, handler);
        *slot.lock() = Some(peer.clone());
        let why = pump.await;
        self.nodes.left(&name, &why);
        // Its plugins stop being offered the moment its socket goes. The
        // interned manifests stay, because a source being torn down may still
        // hold one.
        crate::plugin::remote::forget(&name);
        (self.watch)(NodeEvent::Left { node: name.clone(), why: why.clone() });
        Ok(())
    }

    fn handler(
        self: Arc<Self>,
        name: String,
        from: SocketAddr,
        identity: String,
        slot: Slot,
    ) -> Handler {
        Arc::new(move |method: String, params: Value| {
            let me = self.clone();
            let name = name.clone();
            let identity = identity.clone();
            let slot = slot.clone();
            let boxed: Answer = Box::pin(async move {
                me.answer(&name, from, &identity, &method, params, &slot).await.map_err(|e| {
                    super::wire::FrameError { code: -32603, message: format!("{e:#}"), data: None }
                })
            });
            boxed
        })
    }

    async fn answer(
        &self,
        name: &str,
        from: SocketAddr,
        identity: &str,
        method: &str,
        params: Value,
        slot: &Slot,
    ) -> Result<Value> {
        match method {
            // The node says what it is and what it has. Its name comes from
            // the certificate, so a hello naming another node is a refusal
            // rather than an impersonation.
            "node.hello" => {
                let hello: Hello =
                    serde_json::from_value(params).context("a hello that did not parse")?;
                anyhow::ensure!(
                    hello.name == name,
                    "this connection's certificate says `{name}` and the hello said `{}`. Enrol \
                     again with the right name",
                    hello.name
                );
                anyhow::ensure!(
                    hello.api == BRIDGE_API,
                    "this node speaks node bridge version {} and the core speaks {BRIDGE_API}. \
                     Upgrade whichever is behind",
                    hello.api
                );
                let peer = slot
                    .lock()
                    .take()
                    .context("a second hello arrived on one connection; the first one won")?;
                // What a node has becomes reachable here rather than in
                // whatever is watching, because a core that did not learn a
                // node's plugins would accept `place = "node:x"` and then have
                // nothing to build. The watcher is told afterwards.
                crate::plugin::remote::learn(
                    name,
                    hello.plugins.clone(),
                    hello.schemas.clone(),
                );
                self.nodes.joined(&hello, Some(identity.to_string()), from.to_string(), peer);
                tracing::info!(
                    node = name,
                    %from,
                    version = %hello.version,
                    plugins = hello.plugins.len(),
                    "a node joined"
                );
                (self.watch)(NodeEvent::Joined {
                    node: name.to_string(),
                    hello: Box::new(hello),
                });
                Ok(serde_json::to_value(Welcome {
                    core: "godwinmix".into(),
                    version: env!("CARGO_PKG_VERSION").into(),
                    api: BRIDGE_API,
                    canvas: self.canvas,
                    clock: self.clock.clone(),
                })?)
            }
            "heartbeat" => {
                let beat: Heartbeat =
                    serde_json::from_value(params).context("a heartbeat that did not parse")?;
                for report in &beat.instances {
                    (self.watch)(NodeEvent::Instance {
                        node: name.to_string(),
                        instance: report.instance.clone(),
                        state: report.state.clone(),
                        detail: report.detail.clone(),
                    });
                }
                self.nodes.beat(name, &beat);
                Ok(json!({}))
            }
            "event" => {
                (self.watch)(NodeEvent::Event {
                    node: name.to_string(),
                    instance: params
                        .get("instance")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    name: params
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("event")
                        .to_string(),
                    params: params.get("params").cloned().unwrap_or(Value::Null),
                });
                Ok(json!({}))
            }
            "log" => {
                (self.watch)(NodeEvent::Log {
                    node: name.to_string(),
                    instance: params
                        .get("instance")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    level: params
                        .get("level")
                        .and_then(Value::as_str)
                        .unwrap_or("info")
                        .to_string(),
                    message: params
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                });
                Ok(json!({}))
            }
            other => anyhow::bail!(
                "the core answers `node.hello`, `heartbeat`, `event` and `log` on the node \
                 bridge, and a node asked for `{other}`"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_watcher_type_is_cheap_to_make() {
        let seen = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let mine = seen.clone();
        let watch: Watcher = Arc::new(move |e| mine.lock().push(format!("{e:?}")));
        watch(NodeEvent::Left { node: "studio-b".into(), why: "the test".into() });
        assert_eq!(seen.lock().len(), 1);
    }
}
