//! What the operator set, read out of the validated settings object.
//!
//! The core has already checked the object against `schemas/service.json`
//! before it arrives, so everything here is a read with a default rather than
//! a validation. The defaults are the ones in the schema, written twice on
//! purpose: a plugin started by hand with no core has no schema to read
//! defaults from.

use serde_json::Value;

/// The listening port every OSC surface offers first. TouchOSC, Open Stage
/// Control, QLab and Companion's generic OSC module all default to it.
pub const DEFAULT_PORT: u16 = 9000;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// Where to listen for OSC, as `address:port`.
    pub listen: String,
    /// Where to send tally and programme, as `address:port`, none by default.
    pub send_to: Vec<String>,
    /// Send `<prefix>/tally/<id>` when the tally changes.
    pub send_tally: bool,
    /// Send `<prefix>/program`, `<prefix>/source/<id>/state` and
    /// `<prefix>/output/<id>/state`.
    pub send_program: bool,
    /// Address prefix on everything sent out. Empty means no prefix.
    pub prefix: String,
    /// Addresses allowed to send. Empty means anyone who can reach the port,
    /// which is the right default on a show LAN and the wrong one anywhere
    /// else, and the schema says so.
    pub allow_from: Vec<String>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            listen: format!("0.0.0.0:{DEFAULT_PORT}"),
            send_to: Vec::new(),
            send_tally: true,
            send_program: true,
            prefix: "/gmx".into(),
            allow_from: Vec::new(),
        }
    }
}

impl Settings {
    pub fn from_value(value: &Value) -> Settings {
        let mut settings = Settings::default();
        if let Some(listen) = value.get("listen").and_then(Value::as_str) {
            settings.listen = with_host(listen);
        }
        if let Some(list) = value.get("send_to").and_then(Value::as_array) {
            settings.send_to = list
                .iter()
                .filter_map(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(with_host)
                .collect();
        }
        if let Some(on) = value.get("send_tally").and_then(Value::as_bool) {
            settings.send_tally = on;
        }
        if let Some(on) = value.get("send_program").and_then(Value::as_bool) {
            settings.send_program = on;
        }
        if let Some(prefix) = value.get("prefix").and_then(Value::as_str) {
            settings.prefix = normalise_prefix(prefix);
        }
        if let Some(list) = value.get("allow_from").and_then(Value::as_array) {
            settings.allow_from = list
                .iter()
                .filter_map(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
        }
        settings
    }

    /// Whether a packet from this address is acted on.
    pub fn allows(&self, peer: &std::net::SocketAddr) -> bool {
        if self.allow_from.is_empty() {
            return true;
        }
        let ip = peer.ip().to_string();
        self.allow_from
            .iter()
            .any(|allowed| allowed == &ip || allowed == &peer.to_string())
    }

    /// An outgoing address with the prefix on the front.
    pub fn address(&self, tail: &str) -> String {
        format!("{}{}", self.prefix, tail)
    }
}

/// A bare port, which is what people type, becomes a bindable address.
fn with_host(text: &str) -> String {
    let text = text.trim();
    if text.contains(':') {
        text.to_string()
    } else if text.chars().all(|c| c.is_ascii_digit()) {
        format!("0.0.0.0:{text}")
    } else {
        format!("{text}:{DEFAULT_PORT}")
    }
}

/// A prefix is either empty or starts with a slash and does not end with one.
fn normalise_prefix(prefix: &str) -> String {
    let trimmed = prefix.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        String::new()
    } else if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_defaults_are_the_ones_in_the_schema() {
        let settings = Settings::from_value(&json!({}));
        assert_eq!(settings, Settings::default());
        assert_eq!(settings.listen, "0.0.0.0:9000");
        assert_eq!(settings.prefix, "/gmx");
        assert!(settings.send_tally && settings.send_program);
    }

    #[test]
    fn a_bare_port_is_read_as_an_address() {
        let settings = Settings::from_value(&json!({"listen": "9010"}));
        assert_eq!(settings.listen, "0.0.0.0:9010");
    }

    #[test]
    fn a_bare_host_gets_the_usual_port() {
        let settings = Settings::from_value(&json!({"send_to": ["10.0.0.5"]}));
        assert_eq!(settings.send_to, vec!["10.0.0.5:9000"]);
    }

    #[test]
    fn an_empty_target_is_dropped_rather_than_bound() {
        let settings = Settings::from_value(&json!({"send_to": ["", "10.0.0.5:9001"]}));
        assert_eq!(settings.send_to, vec!["10.0.0.5:9001"]);
    }

    #[test]
    fn a_prefix_is_normalised_both_ways() {
        for (given, want) in [
            ("gmx", "/gmx"),
            ("/gmx/", "/gmx"),
            ("", ""),
            ("  /mixer ", "/mixer"),
        ] {
            let settings = Settings::from_value(&json!({ "prefix": given }));
            assert_eq!(settings.prefix, want, "prefix {given:?}");
        }
    }

    #[test]
    fn an_empty_allow_list_allows_everyone() {
        let settings = Settings::default();
        assert!(settings.allows(&"10.0.0.9:5000".parse().unwrap()));
    }

    #[test]
    fn an_allow_list_matches_on_the_address_with_or_without_the_port() {
        let settings = Settings::from_value(&json!({"allow_from": ["10.0.0.9", "10.0.0.8:41"]}));
        assert!(settings.allows(&"10.0.0.9:5000".parse().unwrap()));
        assert!(settings.allows(&"10.0.0.8:41".parse().unwrap()));
        assert!(!settings.allows(&"10.0.0.8:42".parse().unwrap()));
        assert!(!settings.allows(&"10.0.0.7:5000".parse().unwrap()));
    }

    #[test]
    fn an_outgoing_address_carries_the_prefix() {
        let settings = Settings::default();
        assert_eq!(settings.address("/tally/cam1"), "/gmx/tally/cam1");
        let bare = Settings::from_value(&json!({"prefix": ""}));
        assert_eq!(bare.address("/tally/cam1"), "/tally/cam1");
    }
}
