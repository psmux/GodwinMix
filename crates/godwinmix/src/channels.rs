//! RTMP channels: the core's half.
//!
//! A channel is a named place encoders publish to on the mixer's own RTMP
//! port (`dev/plans/channels-contract.md`). The listener is the ingest
//! plugin's `ingest/discover`; this module owns everything else. It keeps the
//! channels and their keys, persists them beside the runtime store, seals the
//! keys in the secret store, hands the listener its table, hears back what
//! goes live, and turns a live stream into a mixer source when the channel
//! says to.
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
mod destinations;
mod edit;
mod events;
mod keys;
mod live;
mod net;
mod reveal;
mod sending;
mod store;
mod view;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;

use godwinmix_core::mixer::MixerHandle;
use godwinmix_core::plugin::supervisor::Supervisor;
use godwinmix_core::scene::server::SceneServer;
use godwinmix_core::secrets::Secrets;
use godwinmix_protocol::channels::{Channel, ChannelList, RtmpInfo};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::types::Event;
use parking_lot::Mutex;
use serde_json::{json, Value};
use tracing::{error, warn};

pub use live::Live;
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
    live: Mutex<Vec<Live>>,
    /// What the listener last said about each destination.
    sending: Mutex<Vec<sending::Sending>>,
    /// Destination edits, one at a time, so two cannot seal over each other.
    edits: Mutex<()>,
    port: AtomicU16,
    plugins: Arc<Supervisor>,
    mixer: MixerHandle,
    scenes: Arc<SceneServer>,
    secrets: &'static Secrets,
}

impl Channels {
    /// Read the channels, hand the listener its table before it starts, and
    /// start listening for what it says.
    pub fn open(
        runtime_store: Option<PathBuf>,
        port: u16,
        plugins: Arc<Supervisor>,
        mixer: MixerHandle,
        scenes: Arc<SceneServer>,
        secrets: &'static Secrets,
    ) -> Arc<Channels> {
        let path = runtime_store.map(|p| store::path_beside(&p));
        let (records, store) = match path.as_deref().map(store::load) {
            None => (Vec::new(), None),
            Some(Ok(records)) => (records, path),
            Some(Err(e)) => {
                error!(error = %format!("{e:#}"), "the channels file would not parse; no channel is served and nothing is written over it");
                mixer.publish_alert(
                    godwinmix_core::state::Severity::Error,
                    format!("the channels file would not parse, so no channel is served: {e:#}. Fix or move it, then restart."),
                );
                (Vec::new(), None)
            }
        };
        let channels = Arc::new(Channels {
            store,
            records: Mutex::new(records),
            live: Mutex::new(Vec::new()),
            sending: Mutex::new(Vec::new()),
            edits: Mutex::new(()),
            port: AtomicU16::new(port),
            plugins,
            mixer,
            scenes,
            secrets,
        });
        channels.hand_over(false);
        events::start(&channels);
        channels
    }

    /// Give the listener the table: at its next start, and now if `now`.
    fn hand_over(&self, now: bool) {
        let records = self.records.lock().clone();
        let table: Vec<Value> = records
            .iter()
            .map(|r| {
                let keys: Vec<Value> = r
                    .keys
                    .iter()
                    .filter_map(|k| {
                        let secret = self.secrets.get(&keys::scope(&r.id), &k.id)?;
                        Some(json!({"id": k.id, "secret": secret}))
                    })
                    .collect();
                json!({
                    "id": r.id,
                    "app": r.app,
                    "enabled": r.enabled,
                    "key_mode": r.key_mode,
                    "keys": keys,
                    "destinations": self.destination_table(r),
                })
            })
            .collect();
        match toml::Value::try_from(Value::Array(table)) {
            Ok(value) => self.plugins.set_extra(PLUGIN, "channels", Some(value)),
            Err(e) => warn!(%e, "the channel table would not convert for the plugin"),
        }
        if now {
            for (instance, answer) in self.plugins.configure_plugin(PLUGIN) {
                if let Err(e) = answer {
                    warn!(%instance, error = %format!("{e:#}"), "the RTMP listener did not take the new channel table");
                }
            }
        }
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
        let records = self.records.lock().clone();
        store::save(path, &records)
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
        ChannelList { channels: records.iter().map(|r| self.view(r)).collect(), rtmp: self.rtmp() }
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

    /// The port every channel shares, and whether anything is listening.
    fn rtmp(&self) -> RtmpInfo {
        let port = self.port.load(Ordering::Relaxed);
        let listening = self.plugins.is_running(PLUGIN);
        let problem = (!listening).then(|| net::why_not_listening(PLUGIN));
        RtmpInfo { port, urls: net::urls(port), listening, problem }
    }
}
