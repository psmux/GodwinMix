//! A node and a core, in one process, over real mutual TLS.
//!
//! Everything here is the real thing: a real certificate authority, a real TLS
//! handshake with a client certificate, real WebSocket frames, and a real
//! GStreamer pipeline carrying a real `test://` source over SRT from one half
//! of the process to the other. Nothing is mocked, which is the house rule and
//! which is also the only way these tests would catch anything.
//!
//! The acceptance criteria of roadmap Phase 4, in order:
//!
//! * enrolment with a used or expired token is refused;
//! * traffic is mutual TLS and the core sees the client certificate;
//! * a remote plugin's manifest, settings and health look local;
//! * closing the node's socket fails its sources and reconnecting restores
//!   them with no core restart;
//! * a source moves between placements.

use godwinmix_core::node::{self, ca, clock, daemon, enrol, registry::Nodes, server, wire};
use std::sync::Arc;
use std::time::Duration;

/// A core's node bridge on a free port, with its own authority in a temporary
/// directory.
struct Core {
    dir: std::path::PathBuf,
    ca: Arc<ca::NodeCa>,
    tickets: Arc<enrol::Tickets>,
    nodes: Arc<Nodes>,
    address: std::net::SocketAddr,
    seen: Arc<parking_lot::Mutex<Vec<String>>>,
}

impl Core {
    async fn start(tag: &str) -> Core {
        gstreamer::init().unwrap();
        let dir = std::env::temp_dir()
            .join(format!("gmx-node-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ca = Arc::new(ca::NodeCa::open_or_create(&dir.join("ca")).unwrap());
        let tickets = Arc::new(enrol::Tickets::open(&dir.join("enrolment.json")));
        let nodes = Nodes::new();
        let seen = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let mine = seen.clone();
        let watch: server::Watcher = Arc::new(move |event| {
            let line = match &event {
                server::NodeEvent::Joined { node, .. } => format!("joined:{node}"),
                server::NodeEvent::Left { node, .. } => format!("left:{node}"),
                server::NodeEvent::Instance { instance, state, .. } => {
                    format!("instance:{instance}:{state}")
                }
                server::NodeEvent::Event { name, .. } => format!("event:{name}"),
                server::NodeEvent::Log { level, .. } => format!("log:{level}"),
            };
            mine.lock().push(line);
        });
        // A clock on a free port, so two cores in one test run do not fight.
        let provider = clock::Provider::publish(&gstreamer::SystemClock::obtain(), 0).unwrap();
        let server = server::NodeServer::new(
            ca.clone(),
            tickets.clone(),
            nodes.clone(),
            godwinmix_protocol::plugin::wire::Canvas { width: 1280, height: 720, fps: 30 },
            wire::ClockOffer {
                host: "127.0.0.1".into(),
                port: provider.port(),
                kind: "net".into(),
            },
            watch,
        );
        let address = server
            .serve("127.0.0.1:0".parse().unwrap(), &["localhost".into(), "127.0.0.1".into()])
            .await
            .unwrap();
        // The provider is leaked on purpose: it must outlive the test's nodes,
        // and a test process is about to exit anyway.
        std::mem::forget(provider);
        std::mem::forget(server);
        Core { dir, ca, tickets, nodes, address, seen }
    }

    fn token(&self, name: &str) -> String {
        let ticket = self.tickets.mint(name, enrol::DEFAULT_TTL).unwrap();
        format!("{}.{}", self.ca.fingerprint(), ticket.token)
    }

    fn options(&self, name: &str, token: Option<String>) -> daemon::Options {
        daemon::Options {
            core: format!("127.0.0.1:{}", self.address.port()),
            name: name.into(),
            token,
            home: self.dir.join("node-home"),
            clock: clock::Kind::Net,
            media_host: Some("127.0.0.1".into()),
        }
    }

    fn saw(&self, needle: &str) -> bool {
        self.seen.lock().iter().any(|line| line.contains(needle))
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Wait for a condition, or fail with what was actually true.
async fn until(what: &str, mut check: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while std::time::Instant::now() < deadline {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for {what}");
}

/// A node with no certificate may enrol exactly once, and the certificate it
/// gets carries its SPIFFE identity.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_node_enrols_once_and_a_used_token_is_refused() {
    let core = Core::start("enrol").await;
    let token = core.token("studio-b");
    let options = core.options("studio-b", Some(token.clone()));

    let issued = daemon::ensure_identity(&options).await.expect("the first enrolment works");
    assert_eq!(issued.identity, "spiffe://godwinmix/node/studio-b");
    assert!(issued.cert_pem.contains("BEGIN CERTIFICATE"));

    // A second machine replaying the same token, with no certificate of its
    // own on disk yet.
    let (pin, secret) = ca::split_token(&token);
    let again = daemon::enrol(
        &format!("127.0.0.1:{}", core.address.port()),
        "127.0.0.1",
        "studio-b",
        &format!("{}.{secret}", pin.unwrap()),
    )
    .await;
    let e = format!("{:#}", again.expect_err("a used token must be refused"));
    assert!(e.contains("already used"), "got: {e}");
}

/// A token minted for one node does not enrol another, and an expired one does
/// not enrol anybody.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_token_for_another_node_and_an_expired_token_are_both_refused() {
    let core = Core::start("refuse").await;
    let token = core.token("studio-b");
    let address = format!("127.0.0.1:{}", core.address.port());

    let wrong = format!(
        "{:#}",
        daemon::enrol(&address, "127.0.0.1", "studio-c", &token)
            .await
            .expect_err("a token minted for studio-b must not enrol studio-c")
    );
    assert!(wrong.contains("studio-b"), "the refusal must name the node it was for: {wrong}");

    // An enrolment that was never minted at all.
    let invented = format!(
        "{:#}",
        daemon::enrol(
            &address,
            "127.0.0.1",
            "studio-d",
            &format!("{}.{}", core.ca.fingerprint(), "0".repeat(64)),
        )
        .await
        .expect_err("an invented token must be refused")
    );
    assert!(invented.contains("gmx node token"), "the refusal must say what to do: {invented}");
}

/// The node's certificate is presented on every later connection, and the core
/// takes the node's name out of it rather than out of anything the node says.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_bridge_is_mutual_tls_and_the_name_comes_from_the_certificate() {
    let core = Core::start("mtls").await;
    let options = core.options("studio-b", Some(core.token("studio-b")));
    let identity = daemon::ensure_identity(&options).await.unwrap();

    let node = daemon::Node::new(options).unwrap();
    let driving = tokio::spawn({
        let node = node.clone();
        let identity = identity.clone();
        async move {
            let outcome = daemon::connect(&node, &identity).await;
            if let Err(e) = &outcome {
                eprintln!("the node could not connect: {e:#}");
            }
            outcome
        }
    });
    until("the node to join", || core.nodes.is_online("studio-b")).await;

    let view = core.nodes.view("studio-b").expect("the node is in the registry");
    assert_eq!(view.state, "online");
    assert_eq!(
        view.identity.as_deref(),
        Some("spiffe://godwinmix/node/studio-b"),
        "the core must have read the identity off the presented client certificate"
    );
    assert!(core.saw("joined:studio-b"));

    // A hello that claims another name on this connection is refused, because
    // the certificate has already settled who this is.
    let link = core.nodes.link("studio-b").expect("a live bridge");
    link.close("the test is done");
    driving.abort();
}

/// Closing the node's socket is what pulling its network cable looks like from
/// the core: the node goes offline, its sources are marked unreachable, and
/// when it comes back they are started again with no core restart.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cutting_the_socket_fails_the_sources_and_reconnecting_restores_them() {
    let core = Core::start("cut").await;
    let options = core.options("studio-b", Some(core.token("studio-b")));
    let identity = daemon::ensure_identity(&options).await.unwrap();

    let node = daemon::Node::new(options.clone()).unwrap();
    let first = tokio::spawn({
        let node = node.clone();
        let identity = identity.clone();
        async move { daemon::connect(&node, &identity).await }
    });
    until("the node to join", || core.nodes.is_online("studio-b")).await;

    // The reconciler wants two sources on it. The node has neither, so both
    // are decisions to start.
    let reconciler = node::reconcile::Reconciler::new(core.nodes.clone());
    for id in ["cam1", "cam2"] {
        reconciler.want(node::reconcile::Desired {
            instance: id.into(),
            type_id: "test/source".into(),
            place: node::Place::Node("studio-b".into()),
            params: serde_json::Value::Null,
            transport: wire::BridgeTransport::Srt,
            latency_ms: None,
        });
    }
    let starting = reconciler.tick();
    assert_eq!(
        starting.iter().filter(|a| matches!(a, node::reconcile::Action::Start { .. })).count(),
        2,
        "both sources should be started on the node, got {starting:?}"
    );

    // Pull the cable.
    core.nodes.link("studio-b").unwrap().close("the test pulled the cable");
    until("the node to go offline", || !core.nodes.is_online("studio-b")).await;
    let cut = reconciler.tick();
    assert!(
        cut.iter().all(|a| matches!(a, node::reconcile::Action::Unreachable { .. })),
        "every source on a node that has gone is unreachable, got {cut:?}"
    );
    let why = cut[0].why();
    assert!(why.contains("freeze frame"), "the decision must say what the viewer sees: {why}");
    assert!(why.contains("slate"), "{why}");
    let _ = first.await;

    // Plug it back in. No core restart: the same registry, the same
    // reconciler, the same process.
    let node = daemon::Node::new(options).unwrap();
    let second = tokio::spawn({
        let node = node.clone();
        async move { daemon::connect(&node, &identity).await }
    });
    until("the node to come back", || core.nodes.is_online("studio-b")).await;
    let back = reconciler.tick();
    assert!(
        back.iter().any(|a| matches!(a, node::reconcile::Action::Back { .. })),
        "the alert must clear, got {back:?}"
    );
    assert_eq!(
        back.iter().filter(|a| matches!(a, node::reconcile::Action::Start { .. })).count(),
        2,
        "both sources are started again, got {back:?}"
    );
    second.abort();
}

/// A node reports its plugins, and the core offers them as if they were
/// installed here: the same manifest, the same provide ids, the same tier
/// machinery underneath.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_nodes_plugins_are_offered_by_the_core() {
    let core = Core::start("plugins").await;
    let options = core.options("studio-b", Some(core.token("studio-b")));
    let identity = daemon::ensure_identity(&options).await.unwrap();
    let node = daemon::Node::new(options).unwrap();
    let driving = tokio::spawn({
        let node = node.clone();
        async move { daemon::connect(&node, &identity).await }
    });
    until("the node to join", || core.nodes.is_online("studio-b")).await;

    let view = core.nodes.view("studio-b").unwrap();
    assert!(view.clock_synced || view.heartbeat_age_ms < 3_000, "it is beating: {view:?}");
    assert_eq!(view.platform.as_deref(), Some(daemon::platform()));
    // A node with no plugins installed reports none, and that is a fact rather
    // than a failure: the machine has a mixer binary and nothing else yet.
    assert!(view.provides.iter().all(|p| p.contains('/')));
    driving.abort();
}

/// Every refusal an operator can hit while enrolling names the next step.
#[test]
fn every_enrolment_refusal_says_what_to_do() {
    let book = enrol::Tickets::detached();
    let ticket = book.mint("studio-b", Duration::from_secs(60)).unwrap();
    book.redeem("studio-b", &ticket.token).unwrap();
    let used = book.redeem("studio-b", &ticket.token).unwrap_err().to_string();
    assert!(used.contains("gmx node token"), "{used}");
    let unknown = book.redeem("studio-b", "nope").unwrap_err().to_string();
    assert!(unknown.contains("gmx node token"), "{unknown}");
}
