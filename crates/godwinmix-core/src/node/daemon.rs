//! `godwinmix node`: the same binary, in its second mode.
//!
//! A node is the core's tier 2 host running on another machine, plus a bridge.
//! It enrols once, keeps one WebSocket to the core, hosts plugins out of its
//! own `~/.godwinmix/plugins` exactly as the core would, slaves its clock to
//! the core's, and encodes each plugin's media once for the trip across.
//!
//! What it deliberately does not have: a compositor, an encoder for the
//! programme, a control server, a web UI. A node mixes nothing. If it did, the
//! guarantee that a plugin behaves the same in all three placements would stop
//! being true the moment somebody added a feature to one side.
//!
//! The order at startup matters and is worth writing down:
//!
//! 1. Find or fetch an identity. Without a certificate a node can say exactly
//!    one thing to the core, and that is `node.enrol`.
//! 2. Connect, say hello with every plugin this machine has.
//! 3. Follow the core's clock and wait for it to sync. No plugin starts first:
//!    a source whose timestamps are on the wrong timeline is worse than a
//!    source that is a second late.
//! 4. Heartbeat every second, and answer whatever the core asks.

use super::bridge::{Answer, Handler, Peer};
use super::ca::{self, Issued};
use super::clock::{self, Follower};
use super::media;
use super::wire::{
    FrameError, Heartbeat, Hello, InstanceReport, MediaPlan, Spawn, Welcome, BRIDGE_API,
    HEARTBEAT_EVERY_MS,
};
use crate::caps::CanvasCaps;
use crate::config::{Config, SourceConfig};
use crate::plugin::source::Source;
use crate::plugin::MediaEnds;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// How long a node waits before dialling the core again, and the ceiling it
/// doubles up to. The same shape the plugin supervisor uses, because a node
/// that reconnects in a tight loop against a core that is down is a node that
/// fills a log file.
const FIRST_RETRY: Duration = Duration::from_secs(1);
const MAX_RETRY: Duration = Duration::from_secs(30);

/// What `godwinmix node` was told on the command line.
#[derive(Debug, Clone)]
pub struct Options {
    /// The core's node bridge, `host:port` or a `wss://` URL.
    pub core: String,
    /// What this machine calls itself. Must match the enrolment token's node.
    pub name: String,
    /// A one time enrolment token. Only needed the first time.
    pub token: Option<String>,
    /// Where the identity, the plugins and the logs live. `~/.godwinmix`.
    pub home: PathBuf,
    /// `net` or `ptp`.
    pub clock: clock::Kind,
    /// The address the core should send or dial media on. Worked out from the
    /// socket when it is not given.
    pub media_host: Option<String>,
}

impl Options {
    /// `host:port` with any scheme and path stripped off.
    pub fn authority(&self) -> String {
        let text = self
            .core
            .trim_start_matches("wss://")
            .trim_start_matches("ws://")
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        let host = text.split('/').next().unwrap_or(text);
        if host.contains(':') {
            host.to_string()
        } else {
            format!("{host}:{}", super::server::DEFAULT_PORT)
        }
    }

    pub fn host(&self) -> String {
        let authority = self.authority();
        authority.rsplit_once(':').map(|(h, _)| h.to_string()).unwrap_or(authority)
    }

    /// Where this node keeps the certificate the core gave it.
    pub fn identity_path(&self) -> PathBuf {
        self.home.join("node").join(format!("{}.json", self.name))
    }
}

/// One plugin instance this node is hosting.
struct Hosted {
    source: Box<dyn Source>,
    /// The source's own pipeline, from `MediaEnds`.
    input: gst::Pipeline,
    /// The pipeline that encodes and sends it to the core.
    send: gst::Pipeline,
    /// How this instance's media is getting to the core. Reported back so the
    /// core can see what the node actually did with the plan it sent.
    plan: MediaPlan,
    state: String,
    detail: Option<String>,
    latency_ms: u32,
}

impl Drop for Hosted {
    fn drop(&mut self) {
        let _ = self.send.set_state(gst::State::Null);
        let _ = self.input.set_state(gst::State::Null);
        let _ = self.source.stop();
    }
}

/// The node's state while it is up.
pub struct Node {
    options: Options,
    config: Config,
    canvas: Mutex<CanvasCaps>,
    clock: Mutex<Option<Arc<Follower>>>,
    hosted: Mutex<BTreeMap<String, Hosted>>,
    encoder: String,
    backends: crate::probe::Backends,
}

impl Node {
    /// Build the node's local half: the catalogue choice it will encode with,
    /// and the plugins it has.
    pub fn new(options: Options) -> Result<Arc<Self>> {
        let config: Config = toml::from_str("").context("the empty default config")?;
        let selection = crate::catalogue::select(&config, None)
            .context("work out what this machine can encode with")?;
        let backends = crate::probe::Backends::from_selection(&selection);
        backends.apply_decoder_ranks();
        let canvas = CanvasCaps::new(&config.canvas);
        tracing::info!(
            encoder = %selection.video_encode.element,
            "this node will encode with its own catalogue choice"
        );
        Ok(Arc::new(Self {
            options,
            config,
            canvas: Mutex::new(canvas),
            clock: Mutex::new(None),
            hosted: Mutex::new(BTreeMap::new()),
            encoder: selection.video_encode.element.clone(),
            backends,
        }))
    }

    /// Every plugin installed on this machine, whole manifests, for the hello.
    fn plugins(&self) -> Vec<godwinmix_protocol::plugin::manifest::Manifest> {
        crate::plugin::loader::list()
            .into_iter()
            .filter(|p| p.live())
            .map(|p| p.manifest)
            .collect()
    }

    fn report(&self) -> Vec<InstanceReport> {
        self.hosted
            .lock()
            .iter()
            .map(|(instance, h)| InstanceReport {
                instance: instance.clone(),
                state: h.state.clone(),
                detail: h
                    .detail
                    .clone()
                    .or_else(|| Some(format!("over {}", h.plan.transport.as_str()))),
                latency_ms: h.latency_ms,
            })
            .collect()
    }

    /// Start one plugin and point its media at the core.
    fn spawn(&self, spawn: Spawn) -> Result<Value> {
        let clock = self.clock.lock().clone();
        if let Some(clock) = &clock {
            anyhow::ensure!(
                clock.synced(),
                "this node's clock has not synced to the core yet, so a source started now would \
                 be on the wrong timeline. It syncs on its own; try again in a moment"
            );
        }
        let canvas = CanvasCaps::new(&crate::config::Canvas {
            width: spawn.canvas.width as i32,
            height: spawn.canvas.height as i32,
            ..self.config.canvas.clone()
        });
        *self.canvas.lock() = canvas.clone();

        let mut cfg = SourceConfig::bare(&spawn.instance, "");
        cfg.type_id = Some(spawn.type_id.clone());
        cfg.params = json_to_params(&spawn.params);
        let request = crate::plugin::source::SourceRequest {
            cfg: &cfg,
            canvas: &canvas,
            backends: &self.backends,
            browser: &self.config.browser,
            allow_exec: self.config.security.allow_exec_sources,
            thumb_fps: 1,
            origin: std::time::Instant::now(),
            overlay: None,
        };
        let mut source = crate::plugin::host::make_source(request)
            .with_context(|| format!("start `{}` on this node", spawn.type_id))?;
        let hello = crate::plugin::Hello {
            instance: spawn.instance.clone(),
            canvas: canvas.clone(),
            api_level: crate::plugin::API_LEVEL,
            params: cfg.effective_params(),
            // The tier the plugin is told about is `Sidecar`, because that is
            // what it is: a process on this machine, spoken to over stdin and
            // stderr. Only the core knows the instance is remote, and that is
            // the guarantee the whole design turns on.
            tier: crate::plugin::Tier::Sidecar,
        };
        let ready = source.initialize(hello).context("the plugin's handshake")?;
        let ends = source.start(&canvas, false).context("building the plugin's pipeline")?;
        let send = self
            .send_pipeline(&spawn.instance, &ends, &spawn.media, &canvas)
            .context("building the send pipeline to the core")?;
        if let Some(clock) = &clock {
            clock::adopt(&ends.pipeline, clock.clock(), None);
            clock::adopt(&send, clock.clock(), None);
        }
        ends.pipeline.set_state(gst::State::Playing).context("starting the plugin's pipeline")?;
        send.set_state(gst::State::Playing).context("starting the send pipeline")?;
        let latency_ms = ready.latency_ms.max(spawn.media.latency_ms);
        self.hosted.lock().insert(
            spawn.instance.clone(),
            Hosted {
                source,
                input: ends.pipeline,
                send,
                plan: spawn.media.clone(),
                state: "running".into(),
                detail: None,
                latency_ms,
            },
        );
        tracing::info!(
            instance = %spawn.instance,
            type_id = %spawn.type_id,
            transport = spawn.media.transport.as_str(),
            target = %spawn.media.target,
            latency_ms,
            "hosting a source for the core"
        );
        Ok(json!({ "instance": spawn.instance, "latency_ms": latency_ms }))
    }

    /// Wire a started source's proxy sinks into an encoder and a network sink.
    fn send_pipeline(
        &self,
        id: &str,
        ends: &MediaEnds,
        plan: &MediaPlan,
        canvas: &CanvasCaps,
    ) -> Result<gst::Pipeline> {
        let pipeline = gst::Pipeline::with_name(&format!("{id}-send"));
        let vsrc = crate::gstutil::make("proxysrc", &format!("{id}-send-vsrc"))?;
        vsrc.set_property("proxysink", &ends.video);
        let asrc = crate::gstutil::make("proxysrc", &format!("{id}-send-asrc"))?;
        asrc.set_property("proxysink", &ends.audio);
        // The same reason the programme branch does this: the send pipeline's
        // latency must not depend on the state of the plugin's own pipeline.
        crate::gstutil::answer_latency_here(&vsrc)?;
        crate::gstutil::answer_latency_here(&asrc)?;
        let vq = crate::gstutil::queue_thread(&format!("{id}-send-vq"))?;
        let aq = crate::gstutil::queue_thread(&format!("{id}-send-aq"))?;
        let bin = media::sender(id, plan, canvas, &self.encoder)?;
        pipeline
            .add_many([&vsrc, &vq, &asrc, &aq, bin.upcast_ref::<gst::Element>()])
            .context("adding the send elements")?;
        gst::Element::link_many([&vsrc, &vq]).context("linking the video proxy")?;
        gst::Element::link_many([&asrc, &aq]).context("linking the audio proxy")?;
        vq.link_pads(Some("src"), bin.upcast_ref::<gst::Element>(), Some("video"))
            .context("linking video into the send bin")?;
        aq.link_pads(Some("src"), bin.upcast_ref::<gst::Element>(), Some("audio"))
            .context("linking audio into the send bin")?;
        Ok(pipeline)
    }

    fn stop(&self, instance: &str, reason: &str) -> Result<Value> {
        match self.hosted.lock().remove(instance) {
            Some(_) => {
                tracing::info!(instance, reason, "stopped a source on this node");
                Ok(json!({ "stopped": true }))
            }
            None => Ok(json!({ "stopped": false, "reason": "it was not running here" })),
        }
    }

    /// Everything the core may ask about one instance goes straight through to
    /// the plugin, which is the whole point of the bridge.
    fn to_instance(&self, method: &str, params: Value) -> Result<Value> {
        let instance = params
            .get("instance")
            .and_then(Value::as_str)
            .context("a frame for an instance with no `instance` in it")?
            .to_string();
        let mut hosted = self.hosted.lock();
        let entry = hosted.get_mut(&instance).with_context(|| {
            format!(
                "this node is not hosting `{instance}`. It is hosting: {}",
                "see the heartbeat's instance list"
            )
        })?;
        match method {
            "health" => {
                let health = entry.source.health();
                entry.state = format!("{:?}", health.state).to_lowercase();
                entry.detail = health.detail.clone();
                Ok(serde_json::to_value(godwinmix_protocol::plugin::wire::Health {
                    state: match health.state {
                        crate::plugin::PluginState::Degraded
                        | crate::plugin::PluginState::Stalled => {
                            godwinmix_protocol::plugin::wire::HealthState::Degraded
                        }
                        crate::plugin::PluginState::Failed => {
                            godwinmix_protocol::plugin::wire::HealthState::Failing
                        }
                        _ => godwinmix_protocol::plugin::wire::HealthState::Ok,
                    },
                    detail: health.detail,
                    latency_ms: Some(entry.latency_ms),
                })?)
            }
            "configure" => {
                let params = json_to_params(params.get("params").unwrap_or(&Value::Null));
                match entry.source.configure(&params)? {
                    crate::plugin::Configure::Applied => Ok(json!({ "applied": true })),
                    crate::plugin::Configure::RestartRequired(reason) => {
                        Ok(json!({ "applied": false, "restart_required": true, "reason": reason }))
                    }
                }
            }
            other => entry.source.call(other, params),
        }
    }

    /// The handler the bridge hands every inbound frame to.
    fn handler(self: &Arc<Self>) -> Handler {
        let me = self.clone();
        Arc::new(move |method: String, params: Value| {
            let me = me.clone();
            let boxed: Answer = Box::pin(async move {
                // Everything below builds pipelines and talks to child
                // processes, which is blocking work. It goes on a blocking
                // thread so the socket keeps being read while a plugin is
                // starting.
                let outcome = tokio::task::spawn_blocking(move || match method.as_str() {
                    "node.spawn" => serde_json::from_value::<Spawn>(params)
                        .context("a spawn that did not parse")
                        .and_then(|s| me.spawn(s)),
                    "node.stop" => {
                        let instance =
                            params.get("instance").and_then(Value::as_str).unwrap_or_default();
                        let reason =
                            params.get("reason").and_then(Value::as_str).unwrap_or("the core said so");
                        me.stop(instance, reason)
                    }
                    "node.plugins" => Ok(json!({ "plugins": me.plugins() })),
                    other => me.to_instance(other, params),
                })
                .await;
                match outcome {
                    Ok(Ok(value)) => Ok(value),
                    Ok(Err(e)) => Err(FrameError {
                        code: -32603,
                        message: format!("{e:#}"),
                        data: None,
                    }),
                    Err(e) => Err(FrameError {
                        code: -32603,
                        message: format!("the node's worker thread did not finish: {e}"),
                        data: None,
                    }),
                }
            });
            boxed
        })
    }
}

/// Send one heartbeat a second until the peer dies.
async fn heartbeats(node: Arc<Node>, peer: Arc<Peer>) {
    let mut tick = tokio::time::interval(Duration::from_millis(HEARTBEAT_EVERY_MS));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        if peer.is_closed() {
            return;
        }
        let (offset_ms, jitter_ms, synced) = match node.clock.lock().as_ref() {
            Some(clock) => {
                let (o, j) = clock.reading();
                (o, j, clock.synced())
            }
            None => (0.0, 0.0, false),
        };
        let beat = Heartbeat {
            ts_unix_ms: super::enrol::now_unix() * 1_000,
            clock_offset_ms: offset_ms,
            clock_jitter_ms: jitter_ms,
            clock_synced: synced,
            instances: node.report(),
        };
        if peer.notify("heartbeat", serde_json::to_value(&beat).unwrap_or(Value::Null)).is_err() {
            return;
        }
    }
}

/// `godwinmix node`: enrol if we must, then stay connected forever.
pub async fn run(options: Options) -> Result<()> {
    let identity = ensure_identity(&options).await?;
    let node = Node::new(options.clone())?;
    let mut wait = FIRST_RETRY;
    loop {
        match connect(&node, &identity).await {
            Ok(why) => {
                tracing::warn!(why, "the bridge to the core closed; reconnecting");
                wait = FIRST_RETRY;
            }
            Err(e) => {
                tracing::warn!(error = %format!("{e:#}"), wait_secs = wait.as_secs(), "could not reach the core");
                wait = (wait * 2).min(MAX_RETRY);
            }
        }
        // Everything this node was hosting is gone with the socket. The core
        // asks for it again when it comes back, which is the reconciler's job
        // and not this loop's.
        node.hosted.lock().clear();
        tokio::time::sleep(wait).await;
    }
}

/// One connection, from the TLS handshake to the socket closing.
async fn connect(node: &Arc<Node>, identity: &Issued) -> Result<String> {
    let authority = node.options.authority();
    let host = node.options.host();
    let tls = ca::client_config(&identity.ca_pem, Some(identity))?;
    let stream = dial(&authority, &host, tls).await?;
    let (peer, pump) = Peer::start(stream, node.handler());
    let hello = Hello {
        name: node.options.name.clone(),
        version: env!("CARGO_PKG_VERSION").into(),
        api: BRIDGE_API,
        platform: platform().into(),
        plugins: node.plugins(),
        media_host: node.options.media_host.clone().unwrap_or_default(),
    };
    let welcome: Welcome = serde_json::from_value(
        peer.call("node.hello", serde_json::to_value(&hello)?).await.context("saying hello")?,
    )
    .context("the core's welcome did not parse")?;
    tracing::info!(
        core = %welcome.version,
        clock = %welcome.clock.kind,
        "joined the core"
    );
    *node.canvas.lock() = CanvasCaps::new(&crate::config::Canvas {
        width: welcome.canvas.width as i32,
        height: welcome.canvas.height as i32,
        ..node.config.canvas.clone()
    });
    let kind = clock::Kind::parse(&welcome.clock.kind).unwrap_or(clock::Kind::Net);
    let clock_host =
        if welcome.clock.host.is_empty() { host.clone() } else { welcome.clock.host.clone() };
    let follower = Follower::follow(&clock_host, welcome.clock.port, kind)?;
    if let Err(e) = follower.wait_for_sync(clock::SYNC_TIMEOUT) {
        tracing::warn!(error = %format!("{e:#}"), "carrying on without a synced clock; sources will be refused until it settles");
    }
    *node.clock.lock() = Some(follower);
    tokio::spawn(heartbeats(node.clone(), peer.clone()));
    let why = pump.await;
    *node.clock.lock() = None;
    Ok(why)
}

/// Load the certificate this node was given, or ask for one.
pub async fn ensure_identity(options: &Options) -> Result<Issued> {
    let path = options.identity_path();
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(found) = serde_json::from_str::<Issued>(&text) {
            tracing::info!(identity = %found.identity, "this node already has a certificate");
            return Ok(found);
        }
    }
    let token = options.token.as_deref().with_context(|| {
        format!(
            "this node has no certificate and no enrolment token. Run `gmx node token --name {}` \
             on the core and pass the string it prints as --token",
            options.name
        )
    })?;
    let issued = enrol(&options.authority(), &options.host(), &options.name, token).await?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("make {}", parent.display()))?;
    }
    write_private(&path, &serde_json::to_string_pretty(&issued)?)?;
    tracing::info!(identity = %issued.identity, path = %path.display(), "enrolled with the core");
    Ok(issued)
}

/// Spend a one time token and come back with a certificate.
///
/// This is the only connection in the life of a node where the core's own
/// certificate is not yet trusted, so the token is the authentication in both
/// directions: the core proves nothing here, and the node hands over a secret
/// that is worth one enrolment and expires.
pub async fn enrol(authority: &str, host: &str, name: &str, token: &str) -> Result<Issued> {
    let (pin, secret) = ca::split_token(token);
    let tls = ca::enrolling_client_config(pin)?;
    let stream = dial(authority, host, tls).await?;
    let (peer, pump) = Peer::start(stream, super::bridge::refuse_all());
    let driving = tokio::spawn(pump);
    let answer = peer
        .call_within(
            "node.enrol",
            json!({ "name": name, "token": secret }),
            Duration::from_secs(20),
        )
        .await
        .context("asking the core to enrol this node")?;
    peer.close("enrolled");
    let _ = driving.await;
    serde_json::from_value(answer).context("the certificate the core sent did not parse")
}

/// Open a TLS WebSocket to the core.
async fn dial<S: AsRef<str>>(
    authority: S,
    host: &str,
    tls: Arc<rustls::ClientConfig>,
) -> Result<
    tokio_tungstenite::WebSocketStream<
        tokio_rustls::client::TlsStream<tokio::net::TcpStream>,
    >,
> {
    let authority = authority.as_ref();
    let tcp = tokio::net::TcpStream::connect(authority)
        .await
        .with_context(|| format!("connect to the core's node bridge at {authority}"))?;
    let _ = tcp.set_nodelay(true);
    let server_name = rustls::pki_types::ServerName::try_from(host.to_string())
        .with_context(|| format!("`{host}` is not a name a certificate can be checked against"))?;
    let stream = tokio_rustls::TlsConnector::from(tls)
        .connect(server_name, tcp)
        .await
        .context(
            "the TLS handshake with the core failed. The core's certificate must name the \
             address this node dials; add it to `server_names` under [nodes] on the core",
        )?;
    let (ws, _) = tokio_tungstenite::client_async(format!("wss://{authority}/node"), stream)
        .await
        .context("the WebSocket handshake with the core failed")?;
    Ok(ws)
}

/// The platform triple, in the spelling a manifest's `platforms` list uses.
pub fn platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-aarch64",
        ("macos", "aarch64") => "macos-aarch64",
        ("macos", "x86_64") => "macos-x86_64",
        ("windows", _) => "windows-x86_64",
        _ => "unknown",
    }
}

fn json_to_params(value: &Value) -> crate::config::Params {
    serde_json::from_value::<crate::config::Params>(value.clone()).unwrap_or_default()
}

fn write_private(path: &std::path::Path, contents: &str) -> Result<()> {
    std::fs::write(path, contents).with_context(|| format!("write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// A frame the node sends the core when a plugin says something. Kept here so
/// the shape is written once.
pub fn forward(peer: &Peer, instance: &str, notice: &str, params: Value) {
    let _ = peer.notify(notice, json!({ "instance": instance, "params": params }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::wire::{BridgeTransport, Frame};

    #[test]
    fn an_address_gets_the_default_port_and_loses_its_scheme() {
        let mut options = Options {
            core: "wss://10.0.0.1/node".into(),
            name: "studio-b".into(),
            token: None,
            home: PathBuf::from("/tmp"),
            clock: clock::Kind::Net,
            media_host: None,
        };
        assert_eq!(options.authority(), "10.0.0.1:8443");
        assert_eq!(options.host(), "10.0.0.1");
        options.core = "127.0.0.1:9999".into();
        assert_eq!(options.authority(), "127.0.0.1:9999");
        assert_eq!(options.host(), "127.0.0.1");
    }

    #[test]
    fn the_identity_is_kept_per_node_name() {
        let options = Options {
            core: "core".into(),
            name: "studio-b".into(),
            token: None,
            home: PathBuf::from("/home/x/.godwinmix"),
            clock: clock::Kind::Net,
            media_host: None,
        };
        assert!(options.identity_path().ends_with("node/studio-b.json"));
    }

    #[test]
    fn this_machine_names_a_platform_a_manifest_would_recognise() {
        assert!(
            godwinmix_protocol::plugin::manifest::PLATFORMS.contains(&platform()),
            "the node reported `{}`, which no manifest can match",
            platform()
        );
    }

    #[test]
    fn a_frame_for_an_instance_without_one_is_refused_by_name() {
        let node = Node::new(Options {
            core: "core".into(),
            name: "studio-b".into(),
            token: None,
            home: PathBuf::from("/tmp"),
            clock: clock::Kind::Net,
            media_host: None,
        });
        let Ok(node) = node else { return };
        let e = node.to_instance("health", json!({})).unwrap_err();
        assert!(e.to_string().contains("instance"), "{e}");
    }

    #[test]
    fn a_frame_shape_is_stable() {
        // The bridge's frame type is shared with the core, so a change here is
        // a change to the wire. This is the reminder.
        let f = Frame::notification("heartbeat", json!({}));
        assert_eq!(f.jsonrpc, "2.0");
        assert!(f.id.is_none());
    }

    #[test]
    fn an_unused_transport_still_has_a_plan() {
        for t in BridgeTransport::ALL {
            assert!(!t.as_str().is_empty());
        }
    }
}
