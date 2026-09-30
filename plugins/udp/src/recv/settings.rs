//! The settings of `udp/source`, and the one address they come down to.

use serde_json::Value;

use crate::address::{self, Endpoint};
use crate::ts::plan::Choice;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub uri: String,
    pub address: String,
    pub port: u16,
    pub interface: String,
    pub source_address: String,
    pub program: u16,
    pub pids: Vec<u16>,
    pub receive_buffer_kb: u32,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            uri: String::new(),
            address: "0.0.0.0".into(),
            port: 5000,
            interface: String::new(),
            source_address: String::new(),
            program: 0,
            pids: Vec::new(),
            receive_buffer_kb: 4096,
        }
    }
}

pub fn string(params: &Value, key: &str) -> Option<String> {
    params.get(key).and_then(Value::as_str).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub fn number(params: &Value, key: &str) -> Option<u64> {
    params.get(key).and_then(Value::as_u64)
}

/// `"256, 0x101"` or `[256, 257]`: both are how people write PIDs.
fn pids(params: &Value) -> Result<Vec<u16>, String> {
    let words: Vec<String> = match params.get("pids") {
        Some(Value::Array(a)) => a.iter().map(|v| v.to_string()).collect(),
        Some(Value::String(s)) => s.split([',', ' ']).map(str::to_string).collect(),
        _ => Vec::new(),
    };
    words
        .iter()
        .map(|w| w.trim())
        .filter(|w| !w.is_empty())
        .map(|w| {
            let n = match w.strip_prefix("0x").or_else(|| w.strip_prefix("0X")) {
                Some(hex) => u16::from_str_radix(hex, 16).ok(),
                None => w.parse().ok(),
            };
            n.filter(|&n| (0x10..0x1FFF).contains(&n)).ok_or_else(|| {
                format!(
                    "'{w}' is not an elementary stream PID. PIDs are 16 to 8190, in decimal \
                     or as 0x100; the source's health lists the ones this feed carries."
                )
            })
        })
        .collect()
}

impl Settings {
    /// Read the params the core hands over; `Err` names the one bad field.
    pub fn from_params(params: &Value) -> Result<Settings, String> {
        let d = Settings::default();
        let s = Settings {
            // A kind added from the picker with no address typed carries its own
            // type id as `uri` (`udp/source`). That is not an address, so the
            // fields below are used instead.
            uri: string(params, "uri").filter(|u| u.contains("://")).unwrap_or(d.uri),
            address: string(params, "address").unwrap_or(d.address),
            port: number(params, "port").and_then(|p| u16::try_from(p).ok()).unwrap_or(d.port),
            interface: string(params, "interface").unwrap_or(d.interface),
            source_address: string(params, "source_address").unwrap_or(d.source_address),
            program: number(params, "program").and_then(|p| u16::try_from(p).ok()).unwrap_or(0),
            pids: pids(params)?,
            receive_buffer_kb: number(params, "receive_buffer_kb")
                .map(|v| v.clamp(64, 262_144) as u32)
                .unwrap_or(d.receive_buffer_kb),
        };
        s.endpoint()?;
        Ok(s)
    }

    /// The address to receive on: `uri` when it is set, the fields otherwise.
    pub fn endpoint(&self) -> Result<Endpoint, String> {
        let mut e = if self.uri.is_empty() {
            address::parse(&format!("udp://{}:{}", bracket(&self.address), self.port))?
        } else {
            address::parse(&self.uri)?
        };
        if e.source.is_none() && !self.source_address.is_empty() {
            e.source = Some(self.source_address.clone());
        }
        if e.source.is_some() && !e.multicast() {
            return Err(format!(
                "a source address only means something for a multicast group, and {} is not \
                 one. Clear source_address, or receive from a group such as 232.1.1.1.",
                e.host
            ));
        }
        Ok(e)
    }

    /// The scheme the operator wrote, for the log and the health line.
    pub fn scheme(&self) -> &'static str {
        if self.uri.to_ascii_lowercase().starts_with("rtp://") { "rtp" } else { "udp" }
    }

    pub fn choice(&self) -> Choice {
        Choice { program: self.program, pids: self.pids.clone() }
    }
}

fn bracket(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') { format!("[{host}]") } else { host.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_fields_make_an_address_and_a_pasted_uri_wins() {
        let s = Settings::from_params(&json!({"address": "239.1.1.1", "port": 5000})).unwrap();
        assert_eq!(s.endpoint().unwrap().display("udp"), "udp://@239.1.1.1:5000");
        let s = Settings::from_params(&json!({"uri": "udp://@239.9.9.9:1234", "port": 5000})).unwrap();
        assert_eq!(s.endpoint().unwrap().port, 1234);
    }

    #[test]
    fn pids_are_read_in_decimal_or_hex_and_nonsense_is_refused() {
        let s = Settings::from_params(&json!({"pids": "256, 0x101"})).unwrap();
        assert_eq!(s.pids, vec![256, 257]);
        let err = Settings::from_params(&json!({"pids": "video"})).unwrap_err();
        assert!(err.contains("16 to 8190"), "{err}");
    }

    #[test]
    fn a_source_address_on_a_unicast_port_is_refused_with_the_fix() {
        let err = Settings::from_params(&json!({"source_address": "10.0.0.9"})).unwrap_err();
        assert!(err.contains("Clear source_address"), "{err}");
    }
}
