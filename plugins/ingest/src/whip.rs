//! WHIP for every channel, through the core's control port.
//!
//! The HTTP half is the core's: it answers `POST /whip/<channel>/<stream>`
//! on the port the page already uses, so a browser or OBS publishing by WHIP
//! needs no port of its own, and hands the offer here as `whip.offer`. This
//! half checks the key (the bearer token) against the channel table, makes a
//! `webrtcbin` for the session, and answers with its SDP. From then on the
//! session's media is a channel stream like any other.
//!
//! # The media ports
//!
//! `webrtcbin` does ICE with libnice, which has no single port mux: it binds
//! a UDP socket per session (per interface it offers a candidate on), and
//! cannot share one port between sessions the way mediamtx's ICE UDP mux
//! does. What it does allow is a range, through the ICE agent's
//! `min-rtp-port` and `max-rtp-port`. So WebRTC media uses a small range from
//! `webrtc_port`, and only while a WHIP session is up: no session, no socket.

#[cfg(feature = "webrtc")]
mod session;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::{json, Value};

use crate::channels::{Admit, Protocol};
use crate::gate::ChannelGate;
use crate::proto::Wants;

/// How many UDP ports WebRTC media may use, from `webrtc_port` on.
pub const PORTS: u16 = 16;

pub struct Sessions {
    gate: Arc<ChannelGate>,
    first_port: u16,
    next: AtomicU64,
    live: Arc<Mutex<BTreeMap<String, Live>>>,
}

struct Live {
    channel: String,
    #[cfg(feature = "webrtc")]
    _session: session::Session,
}

impl Sessions {
    pub fn new(gate: Arc<ChannelGate>, first_port: u16) -> Sessions {
        Sessions { gate, first_port, next: AtomicU64::new(1), live: Arc::default() }
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, Live>> {
        self.live.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `whip.offer {app, stream, key, sdp, peer}`: let the publisher in, or
    /// say why not. Answers `{session, sdp}`.
    pub fn offer(&self, params: &Value) -> Result<Value, (i32, String)> {
        let text = |k: &str| params.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        let (app, stream, key, peer) = (text("app"), text("stream"), text("key"), text("peer"));
        let asked = if key.is_empty() { stream.clone() } else { format!("{stream}?psk={key}") };
        let admit = {
            let table = self.gate.table.read().unwrap_or_else(|e| e.into_inner());
            table.admit_via(Protocol::Whip, &app, &asked)
        };
        let admit = admit.map_err(|r| {
            let code = if r.why.contains("no channel called") { 404 } else { 403 };
            (code, self.gate.turn_away(&r.channel, &r.stream, &peer, r.why))
        })?;
        // A session that has gone quiet is taken over by this one rather than
        // refused; see `hub::takeover`.
        if self.gate.hub.held(&admit.app, &admit.stream) {
            let why = format!("{}/{} is already live. Publish under another stream name, or stop the other one first.", admit.app, admit.stream);
            return Err((409, self.gate.turn_away(&admit.channel, &admit.stream, &peer, why)));
        }
        let id = format!("s{}", self.next.fetch_add(1, Ordering::Relaxed));
        self.start(id, admit, &text("sdp"), &peer)
    }

    #[cfg(feature = "webrtc")]
    fn start(&self, id: String, admit: Admit, sdp: &str, peer: &str) -> Result<Value, (i32, String)> {
        let channel = admit.channel.clone();
        let ports = (self.first_port, self.first_port.saturating_add(PORTS - 1));
        let (reaper, name) = (Arc::downgrade(&self.live), id.clone());
        let ended = move || {
            if let Some(live) = reaper.upgrade() {
                let gone = live.lock().unwrap_or_else(|e| e.into_inner()).remove(&name);
                drop(gone);
            }
        };
        let (session, answer) = session::Session::start(&self.gate, admit, sdp, peer, ports, Box::new(ended)).map_err(|e| (400, e))?;
        self.lock().insert(id.clone(), Live { channel, _session: session });
        Ok(json!({"session": id, "sdp": answer}))
    }

    #[cfg(not(feature = "webrtc"))]
    fn start(&self, _id: String, _admit: Admit, _sdp: &str, _peer: &str) -> Result<Value, (i32, String)> {
        Err((501, "this build of the ingest plugin has no WebRTC (it was built without the webrtc feature). Publish by RTMP or SRT.".into()))
    }

    /// `whip.end {session}`: the publisher said it is done (HTTP DELETE).
    pub fn end(&self, id: &str) -> bool {
        let gone = self.lock().remove(id);
        gone.is_some()
    }

    /// The WebRTC media row for `channel.list`: open while a session is up.
    pub fn rows(&self) -> Vec<Value> {
        let live = self.lock();
        let table = self.gate.table.read().unwrap_or_else(|e| e.into_inner());
        let wanted = Wants::of(&table, false).whip;
        if wanted.is_empty() && live.is_empty() {
            return Vec::new();
        }
        let mut because: Vec<String> = live.values().map(|l| l.channel.clone()).collect();
        because.dedup();
        let mut row = Wants::row("webrtc", "udp", self.first_port, !live.is_empty(), &because);
        row["last_port"] = json!(self.first_port.saturating_add(PORTS - 1));
        row["sessions"] = json!(live.len());
        vec![row]
    }
}
