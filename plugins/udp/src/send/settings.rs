//! The settings of `udp/output`.

use serde_json::Value;

use crate::address::{self, Endpoint};
use crate::recv::settings::{number, string};

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub uri: String,
    pub interface: String,
    pub ttl: u32,
    pub packets_per_datagram: u32,
    /// 0 sends what the programme makes. Anything else is a constant rate,
    /// made up with null packets, for receivers that need one.
    pub cbr_kbps: u32,
    /// DiffServ code point, 0 to 63, or -1 to leave the system default.
    pub dscp: i32,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            uri: String::new(),
            interface: String::new(),
            ttl: 8,
            packets_per_datagram: 7,
            cbr_kbps: 0,
            dscp: -1,
        }
    }
}

impl Settings {
    pub fn from_params(params: &Value) -> Result<Settings, String> {
        let d = Settings::default();
        let s = Settings {
            uri: string(params, "uri").filter(|u| u.contains("://")).unwrap_or(d.uri),
            interface: string(params, "interface").unwrap_or(d.interface),
            ttl: number(params, "ttl").map(|v| v.clamp(1, 255) as u32).unwrap_or(d.ttl),
            packets_per_datagram: number(params, "packets_per_datagram")
                .map(|v| v.clamp(1, 7) as u32)
                .unwrap_or(d.packets_per_datagram),
            cbr_kbps: number(params, "cbr_kbps").map(|v| v.min(1_000_000) as u32).unwrap_or(0),
            dscp: params.get("dscp").and_then(Value::as_i64).map(|v| v.clamp(-1, 63) as i32).unwrap_or(-1),
        };
        s.endpoint()?;
        Ok(s)
    }

    /// Where to send. A source address has no meaning when sending, and a
    /// port of 0 has nowhere to go, so both are refused with the fix.
    pub fn endpoint(&self) -> Result<Endpoint, String> {
        if self.uri.is_empty() {
            return Err("there is no address to send to. Set uri to udp://239.1.1.1:5000 for a \
                        multicast group, or udp://192.168.1.50:5000 for one receiver."
                .into());
        }
        let e = address::parse(&self.uri)?;
        if e.source.is_some() {
            return Err(format!(
                "'{}' names a source address, which only a receiver uses. Write the group \
                 alone: udp://{}:{}.",
                self.uri, e.host, e.port
            ));
        }
        if e.port == 0 || e.host == "0.0.0.0" {
            return Err(format!(
                "'{}' does not say where to send. Give the receiver's address or a multicast \
                 group, and a port: udp://239.1.1.1:5000.",
                self.uri
            ));
        }
        Ok(e)
    }

    pub fn rtp(&self) -> bool {
        self.uri.to_ascii_lowercase().starts_with("rtp://")
    }

    /// "udp://@239.1.1.1:5000, TTL 8, CBR 10000 kbit/s".
    pub fn describe(&self) -> String {
        let Ok(e) = self.endpoint() else { return self.uri.clone() };
        let mut words = e.display(if self.rtp() { "rtp" } else { "udp" });
        if e.multicast() {
            words = format!("{words}, TTL {}", self.ttl);
        }
        if !self.interface.is_empty() {
            words = format!("{words}, on {}", self.interface);
        }
        if self.cbr_kbps > 0 {
            words = format!("{words}, CBR {} kbit/s", self.cbr_kbps);
        }
        words
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_defaults_are_seven_packets_and_no_padding() {
        let s = Settings::from_params(&json!({"uri": "udp://239.1.1.1:5000"})).unwrap();
        assert_eq!((s.packets_per_datagram, s.cbr_kbps, s.ttl), (7, 0, 8));
        assert_eq!(s.describe(), "udp://@239.1.1.1:5000, TTL 8");
    }

    #[test]
    fn an_address_with_nowhere_to_go_is_refused_with_an_example() {
        let err = Settings::from_params(&json!({"uri": "udp://@:5000"})).unwrap_err();
        assert!(err.contains("udp://239.1.1.1:5000"), "{err}");
        assert!(Settings::from_params(&json!({})).unwrap_err().contains("no address"));
    }
}
