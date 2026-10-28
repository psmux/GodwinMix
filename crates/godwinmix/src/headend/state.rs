//! What the headend holds while it runs, and starting it.

use super::keep::Keeper;
use super::Settings;
use crate::station::state::Station;
use anyhow::{Context, Result};
use godwinmix_flussonic::api::Live;
use godwinmix_flussonic::facade::access::{At, Segments, Tracks};
use godwinmix_flussonic::import::HLS_OUTPUT;
use godwinmix_flussonic::log::Log;
use godwinmix_flussonic::Document;
use godwinmix_protocol::scope::Token;
use parking_lot::{Mutex, RwLock};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

pub struct Headend {
    pub st: Arc<Station>,
    /// Flussonic's config, as last read and applied. None for a headend set
    /// up on its own screen, with no Flussonic file at all.
    pub file: Option<PathBuf>,
    /// Where Flussonic's URLs and API answer, as bound.
    pub addrs: Vec<String>,
    /// The Flussonic style log, when one is written.
    pub log_path: Option<PathBuf>,
    pub doc: Mutex<Document>,
    /// Every stream of the file that runs as a show, by Flussonic's name.
    pub keep: Keeper,
    /// What each stream is doing, by name, a second old at most.
    pub live: RwLock<BTreeMap<String, Live>>,
    /// The tracks each stream's output serves, a few seconds old at most.
    pub tracks: RwLock<BTreeMap<String, Tracks>>,
    pub segments: Arc<dyn Segments>,
    pub log: Option<Mutex<Log>>,
    /// The station's own methods are called with this.
    pub token: Token,
    /// Rung after the API wrote the file, so it is applied now.
    pub changed: tokio::sync::Notify,
}

impl Headend {
    /// Where stream `name`'s HLS output is, when it runs: a stream of the
    /// file, or a channel kept by the station.
    pub fn at(&self, name: &str) -> Option<At> {
        let id = self.keep.id_of(name).or_else(|| super::native::Native::of(&self.st).keep.id_of(name))?;
        Some(At { show: id, output: HLS_OUTPUT.to_string() })
    }

    /// Every channel that runs as a show, the file's and the station's, by
    /// name: its show and whether it is a copy.
    pub fn shows(&self) -> Vec<(String, String, bool)> {
        let native = super::native::Native::of(&self.st);
        let mine = self.keep.running.lock().iter().map(|(n, a)| (n.clone(), a.id.clone(), a.plan.copy)).collect::<Vec<_>>();
        let theirs = native.keep.running.lock().iter().map(|(n, a)| (n.clone(), a.id.clone(), a.plan.copy)).collect::<Vec<_>>();
        mine.into_iter().chain(theirs).collect()
    }

    /// The file, for a message.
    pub fn file_text(&self) -> String {
        self.file.as_ref().map_or_else(|| "no Flussonic file".to_string(), |f| f.display().to_string())
    }

    pub fn log_line(&self, stream: &str, e: godwinmix_flussonic::log::Event) {
        if let Some(log) = &self.log {
            if let Err(err) = log.lock().event(stream, e) {
                tracing::warn!(error = %err, "could not write the Flussonic style log");
            }
        }
    }
}

fn read(file: &std::path::Path) -> Result<Document> {
    let text = std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    Document::parse(&text).map_err(|e| anyhow::anyhow!("{}: {e}", file.display()))
}

/// Read the file, bind its port, make its shows, and keep them in step.
/// With no file, bind the port asked for and serve the station's own
/// channels on it.
pub async fn start(st: Arc<Station>, s: Settings) -> Result<()> {
    let doc = match &s.file {
        Some(f) => read(f)?,
        None => Document::parse("").map_err(|e| anyhow::anyhow!("an empty config: {e}"))?,
    };
    // With no file, Flussonic's URLs answer only where a port was asked for.
    let listeners = match (&s.file, s.port) {
        (None, None) => Vec::new(),
        _ => super::facade::listeners(&doc.config, s.port, s.address.as_deref()),
    };
    let mut bound = Vec::new();
    for addr in &listeners {
        let l = tokio::net::TcpListener::bind(addr).await.with_context(|| format!("binding {addr} for Flussonic's URLs and API"))?;
        bound.push(l);
    }
    let keep = Keeper::new(&st, super::saved::FLUSSONIC);
    let h = Arc::new(Headend {
        segments: Arc::new(super::ts_bridge::Ts::new(st.clone())),
        st,
        file: s.file.clone(),
        addrs: bound.iter().filter_map(|l| l.local_addr().ok()).map(|a| a.to_string()).collect(),
        log_path: s.log.clone(),
        doc: Mutex::new(doc),
        keep,
        live: RwLock::new(BTreeMap::new()),
        tracks: RwLock::new(BTreeMap::new()),
        log: s.log.as_deref().map(|p| Mutex::new(Log::new(p))),
        token: Token::open(),
        changed: tokio::sync::Notify::new(),
    });
    // The station's own methods read it for headend.status. Weak, since the
    // headend holds the station.
    let _ = h.st.headend.set(Arc::downgrade(&h));
    for l in bound {
        let addr = l.local_addr()?;
        info!(%addr, file = %h.file_text(), "headend: Flussonic's URLs and API");
        eprintln!("Headend: Flussonic's URLs and API on http://{addr}/, from {}.", h.file_text());
        let app = super::facade::router(h.clone());
        tokio::spawn(async move {
            let made = app.into_make_service_with_connect_info::<std::net::SocketAddr>();
            if let Err(e) = axum::serve(l, made).await {
                tracing::error!(error = %e, "the headend's port stopped");
            }
        });
    }
    // Beside the port, not before it: an install from a checkout builds
    // the plugin first, and the API answers meanwhile.
    let (st, settings) = (h.st.clone(), s.clone());
    tokio::spawn(async move { super::ingest::ensure(&st, &settings).await });
    super::native::Native::of(&h.st).start();
    if h.file.is_some() {
        super::apply::apply_file(&h, true).await;
        tokio::spawn(super::watch::watch(h.clone()));
    }
    tokio::spawn(super::live::keep(h.clone()));
    Ok(())
}
