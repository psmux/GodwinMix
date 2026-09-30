//! One stream being converted: a reader of the hub, the pipeline built
//! while its publisher is live, and the pairs its destinations read.
//!
//! ```text
//!   hub reader ──► thread ──┬──► Input ──► appsrc ─ decode ─ scale ─ encode ─ appsink ─┐
//!                           └──► Router (the stream's own tags) ◄──────────────────────┘
//!                                   └──► renditions hub ──► restream senders
//! ```
//!
//! The reader is an ordinary hub reader with a bounded queue: if the decoder
//! cannot keep up, the thread waits on it, the queue fills, and whole GOPs
//! are dropped there, counted, and the publisher never notices. There is one
//! reader and one decode however many destinations the stream converts for.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::graph::Graph;
use super::input::Input;
use super::router::{Output, Router};
use super::sink::Route;
use super::spec::NodeSpec;
use crate::hub::{Hub, Recv};
use crate::media_tag::TagKind;

const LOOK: Duration = Duration::from_millis(250);

struct Shared {
    router: Arc<Router>,
    input: Input,
    graph: Mutex<Option<Graph>>,
    nodes: Mutex<Vec<NodeSpec>>,
    /// Nodes that would not run in this session or the last, and why.
    failed: Mutex<HashMap<String, String>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct Session {
    stop: Arc<AtomicBool>,
    shared: Arc<Shared>,
}

impl Session {
    pub fn start(hub: Hub, renditions: Hub, app: &str, stream: &str, nodes: Vec<NodeSpec>, outputs: Vec<Output>) -> Session {
        let router = Arc::new(Router::new(renditions, app));
        router.set_nodes(&nodes.iter().map(|n| (n.id.clone(), n.raw.clone())).collect::<Vec<_>>());
        router.set_outputs(outputs);
        let shared = Arc::new(Shared { router, input: Input::default(), graph: Mutex::default(), nodes: Mutex::new(nodes), failed: Mutex::default() });
        let stop = Arc::new(AtomicBool::new(false));
        let (me, halt, app, stream) = (shared.clone(), stop.clone(), app.to_string(), stream.to_string());
        let _ = std::thread::Builder::new().name(format!("gmx-transcode-{stream}")).spawn(move || run(&me, &halt, &hub, &app, &stream));
        Session { stop, shared }
    }

    /// New nodes or new pairs, applied to the running pipeline at once.
    pub fn update(&self, nodes: Vec<NodeSpec>, outputs: Vec<Output>) {
        let s = &self.shared;
        s.router.set_nodes(&nodes.iter().map(|n| (n.id.clone(), n.raw.clone())).collect::<Vec<_>>());
        *lock(&s.nodes) = nodes.clone();
        if let Some(graph) = lock(&s.graph).as_mut() {
            let route: Arc<dyn Route> = s.router.clone();
            graph.apply(&nodes, &s.input, &route);
            lock(&s.failed).extend(graph.failed.clone());
        }
        s.router.set_outputs(outputs);
    }

    /// Why a node is not producing, if it is not.
    pub fn failed(&self, node: &str) -> Option<String> {
        lock(&self.shared.failed).get(node).cloned()
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run(s: &Shared, stop: &AtomicBool, hub: &Hub, app: &str, stream: &str) {
    if let Err(e) = gmx_netkit::init() {
        lock(&s.failed).insert("*".into(), e);
        return;
    }
    while !stop.load(Ordering::Relaxed) {
        let reader = hub.subscribe(app, stream);
        let mut base: Option<u32> = None;
        let mut looked = Instant::now();
        loop {
            let tag = match reader.recv_timeout(LOOK) {
                Recv::Tag(t) => Some(t),
                Recv::Ended => break,
                Recv::Timeout => None,
            };
            if stop.load(Ordering::Relaxed) {
                break;
            }
            if looked.elapsed() >= Duration::from_secs(1) {
                looked = Instant::now();
                errors(s);
            }
            let Some(tag) = tag else { continue };
            if lock(&s.graph).is_none() {
                begin(s);
            }
            let header = tag.sequence_header || tag.kind == TagKind::Script;
            if base.is_none() && !header {
                base = Some(tag.timestamp_ms);
                s.router.set_base(tag.timestamp_ms);
                s.router.start();
            }
            s.input.push(&tag, base.unwrap_or(tag.timestamp_ms));
            s.router.source(stream, tag);
        }
        // The publisher left, or the stream is no longer converted.
        s.router.stop();
        lock(&s.graph).take();
        s.input.reset();
    }
}

/// A session's pipeline, built from the nodes as they are now.
fn begin(s: &Shared) {
    let mut graph = Graph::new();
    let route: Arc<dyn Route> = s.router.clone();
    let nodes = lock(&s.nodes).clone();
    graph.apply(&nodes, &s.input, &route);
    *lock(&s.failed) = graph.failed.clone();
    *lock(&s.graph) = Some(graph);
}

fn errors(s: &Shared) {
    if let Some(graph) = lock(&s.graph).as_mut() {
        for (node, why) in graph.errors() {
            eprintln!("transcode: {node}: {why}");
            lock(&s.failed).insert(node, why);
        }
    }
}
