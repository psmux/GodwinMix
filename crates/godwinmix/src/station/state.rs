//! What the station holds: the list, each show's process, and the pieces it
//! owns for the whole machine.

use super::registry::Registry;
use godwinmix_core::mixer::MixerHandle;
use godwinmix_protocol::scope::Tokens;
use godwinmix_protocol::shows::ShowState;
use parking_lot::Mutex;
use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};
use tokio::sync::watch;

/// One show's process, as the supervisor keeps it.
pub struct Proc {
    pub state: ShowState,
    /// Its control socket, once it has said hello. The relay waits on this.
    pub addr: watch::Sender<Option<SocketAddr>>,
    pub pid: Option<u32>,
    pub restarts: u32,
    pub error: Option<String>,
    /// What its hello must carry. New for every start.
    pub secret: String,
    /// Tells its supervising task to stop it. None while no task runs.
    pub stop: Option<watch::Sender<bool>>,
}

impl Proc {
    pub fn new() -> Proc {
        Proc {
            state: ShowState::Stopped,
            addr: watch::channel(None).0,
            pid: None,
            restarts: 0,
            error: None,
            secret: String::new(),
            stop: None,
        }
    }
}

impl Default for Proc {
    fn default() -> Self {
        Self::new()
    }
}

/// How a show's process is started: this binary, and what it passes on.
#[derive(Debug, Clone)]
pub struct Launch {
    pub exe: PathBuf,
    /// Flags every show gets, such as `--log-format json` and `--rehearsal`.
    pub common: Vec<String>,
}

pub struct Station {
    pub registry: Mutex<Registry>,
    pub procs: Mutex<BTreeMap<String, Proc>>,
    /// Where station events go: every client of every show hears them.
    pub events: MixerHandle,
    pub tokens: Arc<Tokens>,
    pub render: godwinmix_core::render::Station,
    pub channels: OnceLock<Arc<crate::channels::Channels>>,
    /// The link's address, for a show's command line.
    pub link: OnceLock<SocketAddr>,
    pub launch: Launch,
    /// For the relay: one pool of connections to the shows' sockets.
    pub http: reqwest::Client,
    /// Shows that have something going out.
    pub on_air: Mutex<BTreeSet<String>>,
    pub sampler: Mutex<godwinmix_host::sampler::Sampler>,
    /// Set once the station is shutting down, so a show that exits is not
    /// started again.
    pub stopping: AtomicBool,
    /// Rung when the station should stop: the only show was shut down on
    /// purpose, which in one process would have stopped everything.
    pub quit: tokio::sync::Notify,
}

impl Station {
    pub fn new(registry: Registry, events: MixerHandle, tokens: Arc<Tokens>, render: godwinmix_core::render::Station, launch: Launch) -> Arc<Station> {
        let procs = registry.records.iter().map(|r| (r.id.clone(), Proc::new())).collect();
        Arc::new(Station {
            registry: Mutex::new(registry),
            procs: Mutex::new(procs),
            events,
            tokens,
            render,
            channels: OnceLock::new(),
            link: OnceLock::new(),
            launch,
            http: reqwest::Client::builder()
                .pool_idle_timeout(std::time::Duration::from_secs(90))
                .build()
                .unwrap_or_default(),
            on_air: Mutex::new(BTreeSet::new()),
            sampler: Mutex::new(godwinmix_host::sampler::Sampler::new()),
            stopping: AtomicBool::new(false),
            quit: tokio::sync::Notify::new(),
        })
    }

    /// The show a client reaches when it names none.
    pub fn first(&self) -> String {
        self.registry.lock().first()
    }

    pub fn state_of(&self, id: &str) -> Option<ShowState> {
        self.procs.lock().get(id).map(|p| p.state)
    }
}
