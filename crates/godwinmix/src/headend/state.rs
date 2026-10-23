//! What the headend holds while it runs, and starting it.

use super::saved::Running;
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
    /// Flussonic's config, as last read and applied.
    pub file: PathBuf,
    pub doc: Mutex<Document>,
    /// Every stream that runs as a show, by Flussonic's name.
    pub running: Mutex<Running>,
    /// What each stream is doing, by name, a second old at most.
    pub live: RwLock<BTreeMap<String, Live>>,
    /// The tracks each stream's output serves, a few seconds old at most.
    pub tracks: RwLock<BTreeMap<String, Tracks>>,
    pub segments: Arc<dyn Segments>,
    pub log: Option<Mutex<Log>>,
    /// The station's own methods are called with this.
    pub token: Token,
    /// One apply at a time: the watcher's and an API edit's.
    pub applying: tokio::sync::Mutex<()>,
    /// Rung after the API wrote the file, so it is applied now.
    pub changed: tokio::sync::Notify,
}

impl Headend {
    /// Where stream `name`'s HLS output is, when it runs.
    pub fn at(&self, name: &str) -> Option<At> {
        let running = self.running.lock();
        let r = running.get(name)?;
        Some(At { show: r.id.clone(), output: HLS_OUTPUT.to_string() })
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
pub async fn start(st: Arc<Station>, s: Settings) -> Result<()> {
    let doc = read(&s.file)?;
    let listeners = super::facade::listeners(&doc.config, s.port, s.address.as_deref());
    let mut bound = Vec::new();
    for addr in &listeners {
        let l = tokio::net::TcpListener::bind(addr).await.with_context(|| format!("binding {addr} for Flussonic's URLs and API"))?;
        bound.push(l);
    }
    let running = super::saved::load(&st);
    let h = Arc::new(Headend {
        segments: Arc::new(super::ts_bridge::Ts::new(st.clone())),
        st,
        file: s.file.clone(),
        doc: Mutex::new(doc),
        running: Mutex::new(running),
        live: RwLock::new(BTreeMap::new()),
        tracks: RwLock::new(BTreeMap::new()),
        log: s.log.as_deref().map(|p| Mutex::new(Log::new(p))),
        token: Token::open(),
        applying: tokio::sync::Mutex::new(()),
        changed: tokio::sync::Notify::new(),
    });
    for l in bound {
        let addr = l.local_addr()?;
        info!(%addr, file = %s.file.display(), "headend: Flussonic's URLs and API");
        eprintln!("Headend: Flussonic's URLs and API on http://{addr}/, from {}.", s.file.display());
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
    super::apply::apply_file(&h, true).await;
    tokio::spawn(super::watch::watch(h.clone()));
    tokio::spawn(super::live::keep(h.clone()));
    Ok(())
}
