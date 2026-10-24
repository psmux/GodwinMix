//! The ports, opened when a channel needs them and closed when none does.
//!
//! Nothing is bound at start. Each time the core hands over the channel
//! table, [`Listeners::apply`] works out what it needs (`src/proto.rs`) and
//! makes the sockets match:
//!
//! | Listener | Open while |
//! |---|---|
//! | RTMP, every interface | a channel that is on has RTMP on |
//! | RTMP port, loopback only | a channel is on but none has RTMP: the mixer's own sources read the hub through it |
//! | SRT, one UDP port | a channel that is on has SRT on |
//! | RTMPS, a port each | a channel that is on has RTMPS on, on that port |
//!
//! WHIP needs no listener here: it arrives on the control port and the core
//! hands the offer over (`src/whip.rs`). A publisher already on air over a
//! protocol that has just been switched off is cut off by the gate, not by
//! closing the port, so it is told why.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::channels::{Protocol, Table, Tls};
use crate::device::Settings;
use crate::gate::{ChannelGate, Via};
use crate::proto::Wants;
use crate::rtmp::Server;
use crate::srt::SrtServer;

pub struct Listeners {
    settings: Settings,
    gate: Arc<ChannelGate>,
    /// The RTMP port actually bound, which is the setting unless that is 0.
    rtmp_port: u16,
    /// The RTMP listener, and whether it is on every interface.
    rtmp: Option<(Server, bool)>,
    srt: Option<SrtServer>,
    rtmps: BTreeMap<u16, Server>,
    /// The certificate the RTMPS listeners were bound with.
    tls: Option<Tls>,
    wants: Wants,
    problems: BTreeMap<String, String>,
    /// Direct shows read through the relay by the station (`direct::readers`).
    pub readers: Vec<String>,
    /// A loopback port of the relay's own, opened when the RTMP port could
    /// not be bound or does not reach this process from 127.0.0.1. Kept
    /// once open, so the relay's address does not move again.
    own_relay: Option<Server>,
    /// Why it was opened, kept for the relay's row.
    relay_why: Option<String>,
}

impl Listeners {
    pub fn new(settings: &Settings, gate: Arc<ChannelGate>) -> Listeners {
        Listeners {
            rtmp_port: settings.rtmp_port,
            settings: settings.clone(),
            gate,
            rtmp: None,
            srt: None,
            rtmps: BTreeMap::new(),
            tls: None,
            wants: Wants::default(),
            problems: BTreeMap::new(),
            readers: Vec::new(),
            own_relay: None,
            relay_why: None,
        }
    }

    /// Open what `table` needs and close what it does not.
    pub fn apply(&mut self, table: &Table) {
        self.wants = Wants::of(table, self.settings.open_door);
        self.wants.relay.extend(self.readers.iter().map(|id| format!("direct.{id}")));
        self.problems.clear();
        if let Some(why) = &self.relay_why {
            self.problems.insert("relay".into(), why.clone());
        }
        self.apply_rtmp();
        self.apply_srt();
        self.apply_rtmps(table.tls.as_ref());
    }

    pub fn rtmp_port(&self) -> u16 {
        self.rtmp_port
    }

    fn apply_rtmp(&mut self) {
        let wide = !self.wants.rtmp.is_empty();
        let wanted = (wide || !self.wants.relay.is_empty()).then_some(wide);
        if self.rtmp.as_ref().map(|(_, w)| *w) == wanted {
            return;
        }
        // Closed before the other is opened: they are the same port number.
        self.rtmp = None;
        let Some(wide) = wanted else { return };
        let bind = if wide { self.settings.bind.clone() } else { "127.0.0.1".to_string() };
        let gate = Arc::new(Via(self.gate.clone(), Protocol::Rtmp));
        match Server::bind(&bind, self.rtmp_port, gate) {
            Ok(server) => {
                self.rtmp_port = server.port();
                self.note(format!("RTMP port {} open on {bind}", self.rtmp_port));
                if server.answers_on_loopback() {
                    if self.own_relay.is_none() {
                        self.gate.set_relay(format!("127.0.0.1:{}", self.rtmp_port));
                    }
                } else {
                    let why = format!(
                        "127.0.0.1:{} reaches another program, not this plugin: another GodwinMix, or an ingest plugin \
                         left running from an earlier start, holds the port on loopback. Publishers on the network still \
                         reach this one; stop the other program to free the port.",
                        self.rtmp_port
                    );
                    self.fail("relay", why.clone());
                    self.relay_why = Some(why);
                    self.relay_of_its_own();
                }
                self.rtmp = Some((server, wide));
            }
            Err(e) => {
                self.fail("rtmp", e);
                // The shows and sources on this machine still read the hub.
                self.relay_of_its_own();
            }
        }
    }

    /// Open the relay on a loopback port of its own, once, and say where.
    fn relay_of_its_own(&mut self) {
        if self.own_relay.is_some() {
            return;
        }
        let gate = Arc::new(Via(self.gate.clone(), Protocol::Rtmp));
        match Server::bind("127.0.0.1", 0, gate) {
            Ok(server) if server.answers_on_loopback() => {
                let at = format!("127.0.0.1:{}", server.port());
                self.note(format!("the relay for this machine's readers is on {at} instead"));
                self.gate.set_relay(at);
                self.own_relay = Some(server);
            }
            Ok(server) => self.fail("relay", format!("the relay's own port {} does not answer from 127.0.0.1 either", server.port())),
            Err(e) => self.fail("relay", e),
        }
    }

    fn apply_srt(&mut self) {
        let wanted = !self.wants.srt.is_empty();
        if wanted == self.srt.is_some() {
            return;
        }
        if !wanted {
            self.srt = None;
            return self.note(format!("SRT port {} closed", self.settings.srt_port));
        }
        match SrtServer::bind(&self.settings.bind, self.settings.srt_port, self.gate.clone()) {
            Ok(server) => {
                self.note(format!("SRT port {}/udp open", server.port()));
                self.srt = Some(server);
            }
            Err(e) => self.fail("srt", e),
        }
    }

    fn apply_rtmps(&mut self, tls: Option<&Tls>) {
        if self.tls.as_ref() != tls {
            // A new certificate: every RTMPS port binds again with it.
            self.rtmps.clear();
            self.tls = tls.cloned();
        }
        self.rtmps.retain(|port, _| self.wants.rtmps.contains_key(port));
        let wanted: Vec<u16> = self.wants.rtmps.keys().copied().filter(|p| !self.rtmps.contains_key(p)).collect();
        if wanted.is_empty() {
            return;
        }
        let config = match self.tls.as_ref().map(crate::rtmp::server_config) {
            Some(Ok(config)) => config,
            Some(Err(e)) => return self.fail("rtmps", e),
            None => return self.fail("rtmps", "RTMPS needs a certificate. Upload one, or make a self signed one, in the channel's settings.".into()),
        };
        for port in wanted {
            let gate = Arc::new(Via(self.gate.clone(), Protocol::Rtmps));
            match Server::bind_tls(&self.settings.bind, port, gate, Some(config.clone())) {
                Ok(server) => {
                    self.note(format!("RTMPS port {port} open"));
                    self.rtmps.insert(port, server);
                }
                Err(e) => self.fail(&format!("rtmps:{port}"), e),
            }
        }
    }

    /// One row per listener the channels have asked for, open or not, for
    /// `channel.list` to say which ports are open and why.
    pub fn rows(&self) -> Vec<Value> {
        let w = &self.wants;
        let wide = self.rtmp.as_ref().is_some_and(|(_, wide)| *wide);
        let mut rows = vec![
            Wants::row("rtmp", "tcp", self.rtmp_port, wide, &w.rtmp),
            Wants::row("srt", "udp", self.settings.srt_port, self.srt.is_some(), &w.srt),
        ];
        if let Some(own) = &self.own_relay {
            let mut relay = Wants::row("relay", "tcp", own.port(), true, &w.relay);
            relay["loopback"] = json!(true);
            rows.push(relay);
        } else if self.rtmp.is_some() && !wide {
            let mut relay = Wants::row("relay", "tcp", self.rtmp_port, true, &w.relay);
            relay["loopback"] = json!(true);
            rows.push(relay);
        }
        for (port, because) in &w.rtmps {
            rows.push(Wants::row("rtmps", "tcp", *port, self.rtmps.contains_key(port), because));
        }
        for row in rows.iter_mut() {
            let key = match row["protocol"].as_str() {
                Some("rtmps") => format!("rtmps:{}", row["port"]),
                other => other.unwrap_or_default().to_string(),
            };
            if let Some(problem) = self.problems.get(&key).or_else(|| self.problems.get(row["protocol"].as_str().unwrap_or(""))) {
                row["problem"] = json!(problem);
            }
        }
        rows
    }

    /// The ports open now, for a health line: `19381/tcp, 19382/udp`.
    pub fn summary(&self) -> String {
        let open: Vec<String> = self
            .rows()
            .iter()
            .filter(|r| r["open"] == true)
            .map(|r| format!("{} {}/{}", r["protocol"].as_str().unwrap_or(""), r["port"], r["transport"].as_str().unwrap_or("")))
            .collect();
        if open.is_empty() { "no port open, no channel needs one".into() } else { open.join(", ") }
    }

    fn note(&self, line: String) {
        if let Some(r) = &self.gate.reporter {
            r.info(line);
        }
    }

    fn fail(&mut self, what: &str, why: String) {
        if let Some(r) = &self.gate.reporter {
            r.warn(why.clone());
        }
        self.problems.insert(what.to_string(), why);
    }
}

#[cfg(test)]
#[path = "listeners_tests.rs"]
mod tests;
