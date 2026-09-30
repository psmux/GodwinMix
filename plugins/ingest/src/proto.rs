//! The ways a publisher can reach a channel, and which listeners the channel
//! table needs open because of them.
//!
//! No listener runs unless a channel uses it (dev/plans/shows-and-renditions.md,
//! Decision 1). The table says which channels are switched on and over what;
//! [`Wants::of`] turns that into the ports that must be open and the channels
//! each is open for, and `src/listeners.rs` makes the sockets match.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::channels::Table;

/// One way in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Protocol {
    Rtmp,
    Rtmps,
    Srt,
    Whip,
}

impl Protocol {
    pub fn parse(name: &str) -> Option<Protocol> {
        match name.trim().to_ascii_lowercase().as_str() {
            "rtmp" => Some(Protocol::Rtmp),
            "rtmps" => Some(Protocol::Rtmps),
            "srt" => Some(Protocol::Srt),
            "whip" | "webrtc" => Some(Protocol::Whip),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Protocol::Rtmp => "rtmp",
            Protocol::Rtmps => "rtmps",
            Protocol::Srt => "srt",
            Protocol::Whip => "whip",
        }
    }

    fn shown(self) -> &'static str {
        match self {
            Protocol::Rtmp => "RTMP",
            Protocol::Rtmps => "RTMPS",
            Protocol::Srt => "SRT",
            Protocol::Whip => "WHIP",
        }
    }

    /// The refusal for a channel that has this protocol switched off.
    pub fn not_taken(self, app: &str) -> String {
        let shown = self.shown();
        format!(
            "the channel '{app}' does not take {shown}. Switch {shown} on in its settings \
             on the mixer's Channels page, or publish over a protocol it has on."
        )
    }
}

/// The certificate and private key RTMPS answers with, as PEM.
#[derive(Debug, Clone, PartialEq)]
pub struct Tls {
    pub cert: String,
    pub key: String,
}

impl Tls {
    /// `tls: {cert, key}` from the plugin's settings, which the core fills.
    pub fn from_params(params: &Value) -> Option<Tls> {
        let tls = params.get("tls")?;
        let text = |k: &str| tls.get(k).and_then(Value::as_str).map(str::to_string).filter(|s| !s.is_empty());
        Some(Tls { cert: text("cert")?, key: text("key")? })
    }
}

/// Which listeners the table needs, and for which channels.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Wants {
    /// The RTMP port on every interface.
    pub rtmp: Vec<String>,
    /// Every channel: the RTMP port on the loopback, for the mixer's own
    /// sources to read the hub through, when nothing needs it wider.
    pub relay: Vec<String>,
    pub srt: Vec<String>,
    pub whip: Vec<String>,
    /// RTMPS by port: each port a person chose, with the channels on it.
    pub rtmps: BTreeMap<u16, Vec<String>>,
}

impl Wants {
    /// What `table` needs open. `open_door` is a person asking for the RTMP
    /// port to take any publisher with no channel at all.
    pub fn of(table: &Table, open_door: bool) -> Wants {
        let mut wants = Wants::default();
        for c in table.channels.iter().filter(|c| c.enabled) {
            let id = c.id.clone();
            wants.relay.push(id.clone());
            for (on, list) in [
                (Protocol::Rtmp, &mut wants.rtmp),
                (Protocol::Srt, &mut wants.srt),
                (Protocol::Whip, &mut wants.whip),
            ] {
                if c.takes(on) {
                    list.push(id.clone());
                }
            }
            if let Some(port) = c.rtmps_port {
                wants.rtmps.entry(port).or_default().push(id);
            }
        }
        if open_door && table.channels.is_empty() {
            wants.rtmp.push("*".into());
        }
        wants
    }

    /// One row of `listeners` in the `streams` answer.
    pub fn row(protocol: &str, transport: &str, port: u16, open: bool, because: &[String]) -> Value {
        json!({"protocol": protocol, "transport": transport, "port": port, "open": open, "because": because})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(channels: Value) -> Table {
        Table::from_params(&json!({ "channels": channels }))
    }

    #[test]
    fn no_channels_want_no_listener_at_all() {
        assert_eq!(Wants::of(&Table::default(), false), Wants::default());
    }

    #[test]
    fn each_listener_is_wanted_by_the_channels_that_switched_it_on() {
        let t = table(json!([
            {"id": "a", "keys": [], "protocols": ["rtmp", "srt"]},
            {"id": "b", "keys": [], "protocols": ["whip"], "rtmps_port": 443},
            {"id": "c", "keys": [], "protocols": ["srt"], "enabled": false},
            {"id": "d", "keys": []},
        ]));
        let w = Wants::of(&t, false);
        assert_eq!(w.rtmp, vec!["a", "d"], "a channel from an older core is RTMP");
        assert_eq!(w.srt, vec!["a"], "a channel switched off opens nothing");
        assert_eq!(w.whip, vec!["b"]);
        assert_eq!(w.rtmps.get(&443), Some(&vec!["b".to_string()]));
        assert_eq!(w.relay, vec!["a", "b", "d"]);
    }

    #[test]
    fn the_open_door_is_asked_for_and_only_with_no_channels() {
        assert_eq!(Wants::of(&Table::default(), true).rtmp, vec!["*"]);
        let t = table(json!([{"id": "a", "keys": [], "protocols": ["srt"]}]));
        assert!(Wants::of(&t, true).rtmp.is_empty());
    }

    #[test]
    fn a_protocol_switched_off_is_refused_with_the_way_to_switch_it_on() {
        let t = table(json!([{"id": "a", "keys": [{"id": "k", "secret": "0123456789ab"}], "protocols": ["rtmp"]}]));
        let why = t.admit_via(Protocol::Srt, "a", "main?psk=0123456789ab").unwrap_err().why;
        assert!(why.contains("does not take SRT") && why.contains("Channels page"), "{why}");
        assert!(t.admit_via(Protocol::Rtmp, "a", "main?psk=0123456789ab").is_ok());
    }
}
