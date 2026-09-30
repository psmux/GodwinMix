//! Channels: the core's half.
//!
//! A channel is a named place encoders publish to, over RTMP, RTMPS, SRT or
//! WHIP with one set of keys (`dev/plans/channels-contract.md`,
//! `dev/plans/shows-and-renditions.md`). The listeners are the ingest
//! plugin's `ingest/discover`, which opens each port only while a channel
//! uses it; WHIP arrives on the control port and is handed to it
//! (`crate::control::whip`). This module owns everything else. It keeps the
//! channels and their keys, persists them beside the runtime store, seals the
//! keys and the RTMPS certificate in the secret store, hands the listener its
//! table, hears back what goes live, and turns a live stream into a mixer
//! source when the channel says to.
//!
//! ```text
//!   channel.* methods ──► Channels ──set_extra + configure──► ingest/discover
//!                            ▲                                   │
//!                            └──── event/channel.* ◄── pump ◄────┘
//!                            │
//!                            ├──► mixer: AddSource / RemoveSource (auto_source)
//!                            └──► event/channel.changed, .removed, .refused
//! ```
//!
//! Every call here may block (a file write, a plugin call of up to five
//! seconds), so the method handlers run it on the blocking pool, and the
//! plugin's events are handled on a thread of this module's own. Nothing here
//! touches a pipeline except through the mixer's command queue.

mod auto;
mod default;
mod destinations;
mod edit;
mod events;
mod handover;
pub(crate) mod keys;
mod live;
mod net;
mod ports;
pub mod project;
mod reveal;
mod sending;
mod store;
pub mod target;
mod tls;
pub(crate) mod transcode;
mod view;
mod whip;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, OnceLock, Weak};

use godwinmix_core::mixer::MixerHandle;
use godwinmix_core::plugin::supervisor::Supervisor;
use godwinmix_core::secrets::Secrets;
use godwinmix_protocol::channels::{CertificateInfo, Channel, ChannelList, RtmpInfo};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::types::Event;
use parking_lot::Mutex;
use serde_json::Value;
use tracing::error;

pub use live::Live;
pub use ports::Ports;
pub use whip::Whip;
pub use store::Record;

/// The plugin that holds the listener.
pub const PLUGIN: &str = "ingest";
/// Its provide, for a tool call to it by name.
pub const PROVIDE: &str = "ingest/discover";

/// Every channel, and what is live on each.
pub struct Channels {
    /// `None` for a core with no config file, and for one whose channels
    /// file would not parse: then nothing is written over it.
    store: Option<PathBuf>,
    records: Mutex<Vec<Record>>,
    /// What RTMPS answers with; the certificate and key are sealed.
    certificate: Mutex<Option<CertificateInfo>>,
    /// What the listener last said about its ports.
    listeners: Mutex<Vec<Value>>,
    /// The ports from the settings, for addresses of listeners not open yet.
    ports: Ports,
    live: Mutex<Vec<Live>>,
    /// What the listener last said about each destination.
    sending: Mutex<Vec<sending::Sending>>,
    /// Destination edits, one at a time, so two cannot seal over each other.
    edits: Mutex<()>,
    port: AtomicU16,
    /// Destinations that asked for a rendition: their plans and tickets.
    transcode: transcode::Transcode,
    /// The table the listener was last handed, so a replan that changes
    /// nothing does not call it.
    handed: Mutex<Option<Value>>,
    /// The renditions' watch thread is running.
    watching: AtomicBool,
    /// The default channel was made once, or never will be: this mixer had
    /// channels of its own before it existed. See `default.rs`.
    default_made: AtomicBool,
    /// Itself, for the watch thread.
    me: OnceLock<Weak<Channels>>,
    plugins: Arc<Supervisor>,
    /// Where events go: this core's clients, or the station's.
    mixer: MixerHandle,
    /// Where a live stream becomes a source. See `target.rs`.
    target: Arc<dyn target::Programme>,
    secrets: &'static Secrets,
}

impl Channels {
    /// Read the channels, hand the listener its table before it starts, and
    /// start listening for what it says.
    pub fn open(
        runtime_store: Option<PathBuf>,
        ports: Ports,
        plugins: Arc<Supervisor>,
        mixer: MixerHandle,
        target: Arc<dyn target::Programme>,
        secrets: &'static Secrets,
    ) -> Arc<Channels> {
        let data_dir = runtime_store.as_deref().and_then(|p| p.parent()).map(|p| p.to_path_buf());
        let path = runtime_store.map(|p| store::path_beside(&p));
        let (stored, store) = match path.as_deref().map(store::load) {
            None => (store::Stored::default(), None),
            Some(Ok(stored)) => (stored, path),
            Some(Err(e)) => {
                error!(error = %format!("{e:#}"), "the channels file would not parse; no channel is served and nothing is written over it");
                mixer.publish_alert(
                    godwinmix_core::state::Severity::Error,
                    format!("the channels file would not parse, so no channel is served: {e:#}. Fix or move it, then restart."),
                );
                (store::Stored::default(), None)
            }
        };
        // A mixer that already had channels when the default arrived is past
        // needing one.
        let made = stored.default_made || !stored.channels.is_empty();
        let channels = Arc::new(Channels {
            store,
            records: Mutex::new(stored.channels),
            certificate: Mutex::new(stored.certificate),
            listeners: Mutex::new(Vec::new()),
            live: Mutex::new(Vec::new()),
            sending: Mutex::new(Vec::new()),
            edits: Mutex::new(()),
            port: AtomicU16::new(ports.rtmp),
            transcode: transcode::Transcode::new(data_dir),
            handed: Mutex::new(None),
            watching: AtomicBool::new(false),
            default_made: AtomicBool::new(made),
            me: OnceLock::new(),
            ports,
            plugins,
            mixer,
            target,
            secrets,
        });
        let _ = channels.me.set(Arc::downgrade(&channels));
        // A show under a station has no channels of its own: it neither makes
        // the default one nor talks to a listener, which is the station's.
        if crate::station::show::under_station() {
            return channels;
        }
        channels.make_default(godwinmix_core::plugin::loader::get(PLUGIN).is_some());
        channels.hand_over(false);
        events::start(&channels);
        channels
    }

    /// Persist, hand the listener the new table, and say what changed.
    fn commit(&self, changed: Option<&str>) -> Result<(), RpcError> {
        self.persist().map_err(|e| RpcError::internal(format!("saving the channels: {e:#}")))?;
        self.hand_over(true);
        if let Some(id) = changed {
            self.announce(id);
        }
        Ok(())
    }

    fn persist(&self) -> anyhow::Result<()> {
        let Some(path) = &self.store else { return Ok(()) };
        let stored = store::Stored {
            channels: self.records.lock().clone(),
            certificate: self.certificate.lock().clone(),
            default_made: self.default_made.load(Ordering::Relaxed),
        };
        store::save(path, &stored)
    }

    /// `event/channel.changed` for one channel.
    fn announce(&self, id: &str) {
        if let Some(channel) = self.channel(id) {
            self.mixer.emit(Event::ChannelChanged { channel: Box::new(channel) });
        }
    }

    /// `channel.list`.
    pub fn list(&self) -> ChannelList {
        self.refresh();
        let records = self.records.lock().clone();
        ChannelList {
            channels: records.iter().map(|r| self.view(r)).collect(),
            rtmp: self.rtmp(),
            listeners: self.listener_rows(),
            hosts: net::hosts(),
            certificate: self.certificate.lock().clone(),
        }
    }

    /// `channel.get`.
    pub fn get(&self, id: &str) -> Result<Channel, RpcError> {
        self.refresh();
        self.channel(id).ok_or_else(|| self.not_found(id))
    }

    fn not_found(&self, id: &str) -> RpcError {
        let ids: Vec<String> = self.records.lock().iter().map(|r| r.id.clone()).collect();
        RpcError::not_found("channel", id, &ids)
    }

    /// The RTMP port, and whether it is open.
    fn rtmp(&self) -> RtmpInfo {
        let port = self.port.load(Ordering::Relaxed);
        let running = self.plugins.is_running(PLUGIN);
        let listening = running && self.rtmp_open();
        // Closed because no channel has RTMP on is not a problem; closed
        // because the plugin is not there, or the port would not bind, is.
        let problem = if running {
            self.listener_rows().into_iter().find(|r| r.protocol == "rtmp").and_then(|r| r.problem)
        } else {
            Some(net::why_not_listening(PLUGIN))
        };
        RtmpInfo { port, urls: net::urls(port), listening, problem }
    }
}
