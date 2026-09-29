//! The channel server's gate: the table decides, the hub carries, the core is
//! told.
//!
//! Three events go to the core, as `event` notifications on stderr:
//!
//! | Event | When |
//! |---|---|
//! | `channel.stream` with `state: "live"` | a publisher was let in, and again when its codecs are first known |
//! | `channel.stream` with `state: "idle"` | it left |
//! | `channel.refused` | a publisher was turned away, with the reason it was given |
//!
//! With no channels at all the listener is the open door `ingest/discover`
//! always was, and each publisher is announced as `ingest.publisher` for the
//! supervisor to add as a source.

use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use godwinmix_sdk::plugin::Reporter;
use serde_json::{json, Value};

use crate::channels::{split_query, Table};
use crate::hub::Hub;
use crate::rtmp::{self, Gate, Inlet, Kick};

mod inlet;
use inlet::{Channelled, Parts, Stream};

pub struct ChannelGate {
    pub hub: Hub,
    pub table: Arc<RwLock<Table>>,
    /// `app` from the settings: the one application the open door takes, or
    /// any when empty. Not used once there are channels.
    pub open_app: String,
    /// `127.0.0.1:<port>`, where a source reads a stream. Set once bound.
    pub relay: OnceLock<String>,
    pub reporter: Option<Reporter>,
    pub on_air: Arc<OnAir>,
}

/// Every publisher let in on a channel, with the key that let it in and a
/// way to cut it off.
#[derive(Default)]
pub struct OnAir {
    next: AtomicU64,
    list: Mutex<Vec<(u64, String, String, Kick)>>,
}

impl OnAir {
    pub(super) fn add(&self, channel: &str, key: &str, kick: Kick) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.lock().push((id, channel.to_string(), key.to_string(), kick));
        id
    }

    pub(super) fn remove(&self, id: u64) {
        self.lock().retain(|(i, ..)| *i != id);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<(u64, String, String, Kick)>> {
        self.list.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl ChannelGate {
    /// After the table changed: cut off whoever it no longer lets in. A key
    /// taken back, a channel switched off or removed, ends that publisher now
    /// rather than at its next reconnect.
    pub fn enforce(&self) {
        let table = self.table.read().unwrap_or_else(|e| e.into_inner());
        let out: Vec<Kick> = self
            .on_air
            .lock()
            .iter()
            .filter(|(_, channel, key, _)| !table.still_admits(channel, key))
            .map(|(.., kick)| kick.clone())
            .collect();
        drop(table);
        for kick in out {
            kick();
        }
    }

    fn event(&self, name: &str, params: Value) {
        if let Some(r) = &self.reporter {
            r.event(name, params);
        }
    }

    fn relay_address(&self) -> String {
        self.relay.get().cloned().unwrap_or_default()
    }

    fn refuse(&self, channel: &str, stream: &str, peer: &str, why: String) -> String {
        if let Some(r) = &self.reporter {
            let name = if stream.is_empty() { "<key>" } else { stream };
            r.warn(format!("refused {peer} on {channel}/{name}: {why}"));
        }
        self.event(
            "channel.refused",
            json!({"id": channel, "stream": stream, "from": peer, "why": why}),
        );
        why
    }

    fn open_door(&self, app_raw: &str, stream_raw: &str, peer: &str) -> Result<Box<dyn Inlet>, String> {
        let (app, _) = split_query(app_raw);
        let (stream, _) = split_query(stream_raw);
        let filter = rtmp::Filter { app: self.open_app.clone(), key: String::new() };
        if !filter.accepts(app, stream) {
            return Err(self.refuse(app, stream, peer, filter.refusal(app, stream)));
        }
        let publication = self
            .hub
            .publish(app, stream, peer, None)
            .map_err(|why| self.refuse(app, stream, peer, why))?;
        let who = rtmp::Publisher { app: app.into(), key: stream.into(), peer: peer.into() };
        if let Some(r) = &self.reporter {
            r.info(format!("{app}/{stream} from {peer} started publishing"));
        }
        self.event(
            "ingest.publisher",
            json!({
                "action": "connected",
                "id": who.slug(),
                "type": "ingest/rtmp",
                "name": format!("{app}/{stream}"),
                "peer": peer,
                "params": {"relay": self.relay_address(), "stream": format!("{app}/{stream}")},
            }),
        );
        Ok(Box::new(Stream { publication: Some(publication), open: Some(who), gate: self.clone_parts() }))
    }

    fn clone_parts(&self) -> Parts {
        Parts { reporter: self.reporter.clone(), relay: self.relay_address() }
    }
}

impl Gate for ChannelGate {
    fn admit(&self, app_raw: &str, stream_raw: &str, peer: &str, kick: Kick) -> Result<Box<dyn Inlet>, String> {
        let decided = {
            let table = self.table.read().unwrap_or_else(|e| e.into_inner());
            if table.is_open() {
                None
            } else {
                Some(table.admit(app_raw, stream_raw))
            }
        };
        let admit = match decided {
            None => return self.open_door(app_raw, stream_raw, peer),
            Some(Err(r)) => return Err(self.refuse(&r.channel, &r.stream, peer, r.why)),
            Some(Ok(admit)) => admit,
        };
        let publication = self
            .hub
            .publish(&admit.app, &admit.stream, peer, Some(admit.key.clone()))
            .map_err(|why| self.refuse(&admit.channel, &admit.stream, peer, why))?;
        if let Some(r) = &self.reporter {
            r.info(format!("{}/{} from {peer} is live on key {}", admit.app, admit.stream, admit.key));
        }
        let parts = self.clone_parts();
        parts.live(&admit.channel, &publication);
        let stream = Stream { publication: Some(publication), open: None, gate: parts };
        let ticket = self.on_air.add(&admit.channel, &admit.key, kick);
        Ok(Box::new(Channelled {
            channel: admit.channel,
            app: admit.app,
            name: admit.stream,
            stream,
            on_air: (self.on_air.clone(), ticket),
        }))
    }

    fn relay(&self, client: TcpStream, first: &[u8]) {
        if let Err(e) = crate::relay::serve(&self.hub, client, first) {
            self.note(e);
        }
    }

    fn note(&self, message: String) {
        if let Some(r) = &self.reporter {
            r.warn(message);
        }
    }
}
