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
//! Every protocol comes through here: RTMP and RTMPS from their listeners,
//! SRT from `src/srt/`, WHIP from `src/whip/`. The table is asked with the
//! protocol, so a channel with SRT switched off turns an SRT caller away with
//! a sentence saying how to switch it on.
//!
//! With no channels at all, and only when a person asked for it with
//! `open_door`, the RTMP port is the open door `ingest/discover` used to be,
//! and each publisher is announced as `ingest.publisher`.

use std::net::TcpStream;
use std::sync::{Arc, OnceLock, RwLock};

use godwinmix_sdk::plugin::Reporter;
use serde_json::{json, Value};

use crate::channels::{split_query, Admit, Protocol, Table};
use crate::hub::Hub;
use crate::rtmp::{self, Gate, Inlet, Kick};

mod inlet;
mod on_air;
use inlet::{Channelled, Parts, Stream};
pub use on_air::OnAir;

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

impl ChannelGate {
    /// After the table changed: cut off whoever it no longer lets in. A key
    /// taken back, a channel switched off or removed, or its protocol switched
    /// off, ends that publisher now rather than at its next reconnect.
    pub fn enforce(&self) {
        let out = {
            let table = self.table.read().unwrap_or_else(|e| e.into_inner());
            self.on_air.not_admitted_by(&table)
        };
        for kick in out {
            kick();
        }
    }

    /// A publisher asking for `app` and `stream` over `via`: let in, with its
    /// tags on their way to the hub, or refused with the sentence it is told.
    pub fn admit_on(
        &self,
        via: Protocol,
        app_raw: &str,
        stream_raw: &str,
        peer: &str,
        kick: Kick,
    ) -> Result<Box<dyn Inlet>, String> {
        let decided = {
            let table = self.table.read().unwrap_or_else(|e| e.into_inner());
            (!table.is_open()).then(|| table.admit_via(via, app_raw, stream_raw))
        };
        match decided {
            None if via == Protocol::Rtmp => self.open_door(app_raw, stream_raw, peer),
            None => {
                let why = "this mixer has no channels yet. Make one on its Channels page.".to_string();
                Err(self.refuse(app_raw, "", peer, why))
            }
            Some(Err(r)) => Err(self.refuse(&r.channel, &r.stream, peer, r.why)),
            Some(Ok(admit)) => self.let_in(via, admit, peer, kick),
        }
    }

    /// Turn a publisher away that the caller decided on itself, the same way
    /// as any other refusal: a log line and `channel.refused`.
    pub fn turn_away(&self, channel: &str, stream: &str, peer: &str, why: String) -> String {
        self.refuse(channel, stream, peer, why)
    }

    /// A publisher the table let in: a session in the hub, the core told, and
    /// a place on the list of who is on air.
    pub fn let_in(&self, via: Protocol, admit: Admit, peer: &str, kick: Kick) -> Result<Box<dyn Inlet>, String> {
        let publication = self
            .hub
            .publish_with(&admit.app, &admit.stream, peer, Some(admit.key.clone()), via.name(), Some(kick.clone()))
            .map_err(|why| self.refuse(&admit.channel, &admit.stream, peer, why))?;
        if let Some(r) = &self.reporter {
            let how = via.name();
            if let Some(old) = publication.took_over() {
                r.info(format!("{}/{}: {old} had sent nothing for {} s, so {peer} took the name over", admit.app, admit.stream, crate::hub::STALE.as_secs()));
            }
            r.info(format!("{}/{} from {peer} is live over {how} on key {}", admit.app, admit.stream, admit.key));
        }
        let parts = self.clone_parts();
        parts.live(&admit.channel, &publication);
        let stream = Stream { publication: Some(publication), open: None, gate: parts };
        let ticket = self.on_air.add(&admit.channel, &admit.key, via, kick);
        Ok(Box::new(Channelled {
            channel: admit.channel,
            app: admit.app,
            name: admit.stream,
            stream,
            on_air: (self.on_air.clone(), ticket),
        }))
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
        self.event("channel.refused", json!({"id": channel, "stream": stream, "from": peer, "why": why}));
        why
    }

    fn open_door(&self, app_raw: &str, stream_raw: &str, peer: &str) -> Result<Box<dyn Inlet>, String> {
        let (app, _) = split_query(app_raw);
        let (stream, _) = split_query(stream_raw);
        let filter = rtmp::Filter { app: self.open_app.clone(), key: String::new() };
        if !filter.accepts(app, stream) {
            return Err(self.refuse(app, stream, peer, filter.refusal(app, stream)));
        }
        let publication = self.hub.publish(app, stream, peer, None).map_err(|why| self.refuse(app, stream, peer, why))?;
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

/// The gate as one RTMP listener sees it: the shared port, or RTMPS on a port
/// of its own, deciding by the protocol it was bound for.
pub struct Via(pub Arc<ChannelGate>, pub Protocol);

impl Gate for Via {
    fn admit(&self, app_raw: &str, stream_raw: &str, peer: &str, kick: Kick) -> Result<Box<dyn Inlet>, String> {
        self.0.admit_on(self.1, app_raw, stream_raw, peer, kick)
    }

    fn relay(&self, client: TcpStream, first: &[u8]) {
        // Only the plain port carries the hub to the mixer's own sources.
        if self.1 == Protocol::Rtmp {
            self.0.relay(client, first);
        }
    }

    fn note(&self, message: String) {
        self.0.note(message);
    }
}

impl Gate for ChannelGate {
    fn admit(&self, app_raw: &str, stream_raw: &str, peer: &str, kick: Kick) -> Result<Box<dyn Inlet>, String> {
        self.admit_on(Protocol::Rtmp, app_raw, stream_raw, peer, kick)
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
