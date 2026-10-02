//! Live data: feeds the mixer fetches, and bindings that write what they
//! carry into a source's params, a graphic's field or a scene parameter.
//!
//! It lives in the control plane, beside the method handlers, rather than in
//! a sidecar. The binary already links reqwest, tokio-tungstenite and a tokio
//! runtime; a sidecar would be a second process holding its own copies of all
//! three, a token, and a connection back to the core, on a Raspberry Pi, to
//! make calls that are a function call here. And a plugin cannot add methods
//! to the protocol, which `feed.*` has to be.
//!
//! What it costs: nothing while no feed exists. No task, no client, no timer:
//! an empty map behind a mutex. Each feed is one tokio task that sleeps
//! between fetches. Nothing here runs on the mixer thread or a GStreamer
//! streaming thread, and a write is the `source.set` handler a client would
//! call, so a text changes in place with no rebuild. A feed that hangs is held
//! by its own timeout in its own task and delays no other.

mod bind;
mod bindings;
mod check;
mod csv;
mod edit;
mod fetch;
mod parse;
mod path;
mod poll;
mod preview;
mod probe;
mod rss;
mod sse;
mod state;
mod status;
mod store;
mod stream;
mod value;
mod write;
mod xml;

pub use probe::test;

use crate::control::AppState;
use godwinmix_core::snapshot::Tracker;
use godwinmix_protocol::feeds::{BindingSpec, FeedSpec};
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What a feed's task writes through: the state a handler would be given.
#[derive(Clone)]
pub struct Ctx {
    pub app: AppState,
    pub snapshots: Arc<Tracker>,
}

/// Every feed and binding this show has.
pub struct Feeds {
    state: Mutex<State>,
    /// `<stem>.feeds.json` beside the config. `None` for a core with no
    /// config file, which keeps its feeds for as long as it runs.
    file: Option<PathBuf>,
    /// The secret store scope prefix: `feed.<show>`.
    scope: String,
}

#[derive(Default)]
struct State {
    feeds: BTreeMap<String, Feed>,
    bindings: BTreeMap<String, Binding>,
}

struct Feed {
    /// With the real header values, which never leave this struct.
    spec: FeedSpec,
    run: state::Run,
    doc: Option<Arc<Value>>,
    task: Option<tokio::task::JoinHandle<()>>,
    wake: Arc<tokio::sync::Notify>,
}

struct Binding {
    spec: BindingSpec,
    written: Option<Value>,
    last_write: Option<String>,
    writes: u64,
    last_error: Option<String>,
    failures: u32,
}

impl Feeds {
    /// Read what was saved beside `config`. Starts nothing.
    pub fn open(config: &Path) -> Arc<Feeds> {
        let file = (!config.as_os_str().is_empty()).then(|| store::path_beside(config));
        let show = crate::station::show::mode().map(|m| m.id.clone()).unwrap_or_else(|| "main".into());
        let feeds = Feeds { state: Mutex::new(State::default()), file, scope: format!("feed.{show}") };
        feeds.load();
        Arc::new(feeds)
    }

    /// Start a task for every feed that is not paused. Called once the
    /// control plane is up. A show with no feeds starts nothing.
    pub fn start(self: &Arc<Self>, ctx: Ctx) {
        let ids: Vec<String> = self.state.lock().feeds.values().filter(|f| !f.spec.paused).map(|f| f.spec.id.clone()).collect();
        for id in ids {
            self.spawn(&ctx, &id);
        }
    }

    /// (Re)start one feed's task, ending any it had.
    fn spawn(self: &Arc<Self>, ctx: &Ctx, id: &str) {
        let mut st = self.state.lock();
        let Some(feed) = st.feeds.get_mut(id) else { return };
        if let Some(old) = feed.task.take() {
            old.abort();
        }
        if feed.spec.paused {
            return;
        }
        let (me, ctx, id, wake) = (self.clone(), ctx.clone(), id.to_string(), feed.wake.clone());
        let kind = check::kind(&feed.spec);
        feed.task = Some(tokio::spawn(async move {
            match kind {
                check::Kind::Polled => poll::run(me, ctx, id, wake).await,
                check::Kind::WebSocket => stream::websocket(me, ctx, id, wake).await,
                check::Kind::Sse => stream::sse(me, ctx, id, wake).await,
            }
        }));
    }
}

impl Drop for Feeds {
    fn drop(&mut self) {
        for feed in self.state.get_mut().feeds.values_mut() {
            if let Some(task) = feed.task.take() {
                task.abort();
            }
        }
    }
}
