//! Which ports the channels have open, and why.
//!
//! The listener reports what it has bound (the `listeners` member of its
//! `streams` answer); this adds the one listener it does not hold, WHIP on
//! the control port, and when the plugin is not running says for every
//! protocol a channel wants that nothing is listening and what to do.

use godwinmix_core::config::Config;
use godwinmix_protocol::channel_ingest::{ChannelProtocol, Listener};
use serde_json::Value;

use super::{net, Channels, Record, PLUGIN};

/// The ports from the settings, so an address can be shown for a listener
/// that is not open yet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ports {
    pub rtmp: u16,
    pub srt: u16,
    pub control: u16,
}

impl Ports {
    pub fn from_config(cfg: &Config) -> Ports {
        let setting = |key: &str, default: u16| {
            cfg.plugins
                .settings
                .get(PLUGIN)
                .and_then(|t| t.get(key))
                .and_then(|v| v.as_integer())
                .and_then(|p| u16::try_from(p).ok())
                .unwrap_or(default)
        };
        let control = cfg.control.bind.rsplit(':').next().and_then(|p| p.parse().ok()).unwrap_or(8080);
        Ports { rtmp: setting("rtmp_port", 1935), srt: setting("srt_port", 9000), control }
    }
}

impl Default for Ports {
    fn default() -> Ports {
        Ports { rtmp: 1935, srt: 9000, control: 8080 }
    }
}

fn ids(records: &[Record], wants: impl Fn(&Record) -> bool) -> Vec<String> {
    records.iter().filter(|r| r.enabled && wants(r)).map(|r| r.id.clone()).collect()
}

fn row(protocol: &str, transport: &str, port: u16, because: Vec<String>) -> Listener {
    Listener {
        protocol: protocol.into(),
        transport: transport.into(),
        port,
        last_port: None,
        open: false,
        because,
        loopback: false,
        problem: None,
    }
}

impl Channels {
    /// Every listener a channel needs, open or not, with the channels each is
    /// for: `channel.list`'s `listeners`.
    pub(super) fn listener_rows(&self) -> Vec<Listener> {
        let records = self.records.lock().clone();
        let takes = |p: ChannelProtocol| move |r: &Record| r.protocols.contains(&p);
        let whip = ids(&records, takes(ChannelProtocol::Whip));
        let running = self.plugins().is_running(PLUGIN);
        let mut rows: Vec<Listener> = if running {
            self.listeners.lock().iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect()
        } else {
            let mut rows = vec![
                row("rtmp", "tcp", self.ports.rtmp, ids(&records, takes(ChannelProtocol::Rtmp))),
                row("srt", "udp", self.ports.srt, ids(&records, takes(ChannelProtocol::Srt))),
            ];
            for r in records.iter().filter(|r| r.enabled && r.rtmps.enabled) {
                rows.push(row("rtmps", "tcp", r.rtmps.port, vec![r.id.clone()]));
            }
            let why = net::why_not_listening(PLUGIN);
            for r in rows.iter_mut().filter(|r| !r.because.is_empty()) {
                r.problem = Some(why.clone());
            }
            rows
        };
        let mut control = row("whip", "tcp", self.ports.control, whip);
        // The control port is open anyway; WHIP is on it while a channel
        // takes WHIP and the plugin that answers it is running.
        control.open = running && !control.because.is_empty();
        if !running && !control.because.is_empty() {
            control.problem = Some(net::why_not_listening(PLUGIN));
        }
        rows.insert(rows.iter().position(|r| r.protocol == "webrtc").unwrap_or(rows.len()), control);
        rows
    }

    /// Is RTMP open on every interface?
    pub(super) fn rtmp_open(&self) -> bool {
        self.listeners.lock().iter().any(|r| r["protocol"] == "rtmp" && r["open"] == true)
    }

    /// Keep what the listener just said about its ports.
    pub(super) fn listener_report(&self, rows: &[Value]) {
        *self.listeners.lock() = rows.to_vec();
    }
}
