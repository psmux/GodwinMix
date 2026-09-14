//! Turning the settings into one `srt://` address, and never printing the
//! passphrase.
//!
//! SRT carries its options in the query string, which is how every other tool
//! spells them (`srt://host:9000?mode=listener&latency=200`). A person who has
//! an address from a hardware encoder pastes it into `uri` and is done. A
//! person filling in a form gets `host`, `port`, `mode`, `latency_ms`,
//! `passphrase` and `stream_id` and this module assembles the same thing.
//! Anything already in `uri` wins, so pasting an address never has a field
//! quietly overwritten behind it.

use serde_json::Value;

/// The settings of `srt/source`, already validated against the schema.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub uri: String,
    pub host: String,
    pub port: u16,
    pub mode: String,
    pub latency_ms: u32,
    pub passphrase: String,
    pub stream_id: String,
    pub auto_reconnect: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            uri: String::new(),
            host: "0.0.0.0".into(),
            port: 9000,
            mode: "listener".into(),
            latency_ms: 125,
            passphrase: String::new(),
            stream_id: String::new(),
            auto_reconnect: true,
        }
    }
}

fn string(params: &Value, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

impl Settings {
    /// Read the params object the core hands over. Anything missing keeps its
    /// default, which is the shape `configure` needs: the full object, not a
    /// diff, but an object that may legally leave optional keys out.
    pub fn from_params(params: &Value) -> Settings {
        let d = Settings::default();
        Settings {
            uri: string(params, "uri").unwrap_or(d.uri),
            host: string(params, "host").unwrap_or(d.host),
            port: params
                .get("port")
                .and_then(Value::as_u64)
                .and_then(|p| u16::try_from(p).ok())
                .unwrap_or(d.port),
            mode: string(params, "mode").unwrap_or(d.mode),
            latency_ms: params
                .get("latency_ms")
                .and_then(Value::as_u64)
                .map(|v| v.min(10_000) as u32)
                .unwrap_or(d.latency_ms),
            passphrase: string(params, "passphrase").unwrap_or(d.passphrase),
            stream_id: string(params, "stream_id").unwrap_or(d.stream_id),
            auto_reconnect: params
                .get("auto_reconnect")
                .and_then(Value::as_bool)
                .unwrap_or(d.auto_reconnect),
        }
    }

    /// What is wrong with these settings, in one sentence naming the next step.
    pub fn problem(&self) -> Option<String> {
        if !self.uri.is_empty() && !self.uri.to_lowercase().starts_with("srt://") {
            return Some(format!(
                "uri is '{}', which is not an srt:// address. Write srt://host:port, \
                 or leave uri empty and set host and port instead.",
                self.uri
            ));
        }
        if !matches!(self.mode.as_str(), "caller" | "listener" | "rendezvous") {
            return Some(format!(
                "mode is '{}'. It is 'caller' (dial out to a sender), 'listener' \
                 (wait for one to dial in) or 'rendezvous' (both, through a firewall).",
                self.mode
            ));
        }
        if self.uri.is_empty() && self.host.is_empty() {
            return Some(
                "there is no address. Set uri to an srt:// address, or set host and port."
                    .into(),
            );
        }
        if !self.passphrase.is_empty() && self.passphrase.chars().count() < 10 {
            return Some(format!(
                "passphrase is {} characters. SRT requires 10 to 79; a shorter one is \
                 refused by the library, not by us.",
                self.passphrase.chars().count()
            ));
        }
        if self.passphrase.chars().count() > 79 {
            return Some("passphrase is over 79 characters, which SRT refuses.".into());
        }
        None
    }

    /// The address `srtsrc` is given, with the query keys this plugin owns
    /// filled in where the caller did not write them itself.
    ///
    /// The passphrase is deliberately not in here. It is set as a property on
    /// the element instead, so no code path can print it by printing an
    /// address. A passphrase inside a pasted `uri` is left where it is and
    /// hidden by [`Settings::redacted`].
    pub fn address(&self) -> String {
        let base = if self.uri.is_empty() {
            format!("srt://{}:{}", self.host, self.port)
        } else {
            self.uri.clone()
        };
        let (head, existing) = match base.split_once('?') {
            Some((h, q)) => (h.to_string(), q.to_string()),
            None => (base, String::new()),
        };
        let mut query: Vec<String> = existing
            .split('&')
            .filter(|p| !p.trim().is_empty())
            .map(|p| p.to_string())
            .collect();
        let has = |query: &[String], key: &str| {
            query.iter().any(|p| {
                p.split_once('=').map(|(k, _)| k.eq_ignore_ascii_case(key)).unwrap_or(false)
            })
        };
        if !has(&query, "mode") {
            query.push(format!("mode={}", self.mode));
        }
        if !has(&query, "latency") {
            query.push(format!("latency={}", self.latency_ms));
        }
        if !self.stream_id.is_empty() && !has(&query, "streamid") {
            query.push(format!("streamid={}", encode(&self.stream_id)));
        }
        if query.is_empty() {
            head
        } else {
            format!("{head}?{}", query.join("&"))
        }
    }

    /// The address with every secret replaced, which is the only form that is
    /// ever logged or returned to a client.
    pub fn redacted(&self) -> String {
        redact(&self.address())
    }
}

/// Replace the value of any query key that carries a secret.
pub fn redact(address: &str) -> String {
    let Some((head, query)) = address.split_once('?') else {
        return address.to_string();
    };
    let parts: Vec<String> = query
        .split('&')
        .map(|p| match p.split_once('=') {
            Some((k, _)) if k.eq_ignore_ascii_case("passphrase") => format!("{k}=<set>"),
            _ => p.to_string(),
        })
        .collect();
    format!("{head}?{}", parts.join("&"))
}

/// Percent encode the characters that would break a query string.
///
/// A passphrase and a stream id are free text and both routinely carry `&`,
/// `=`, `#` and spaces. Three lines here beat a URL crate in the tree.
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_defaults_wait_for_a_sender_on_9000() {
        let s = Settings::from_params(&json!({}));
        assert_eq!(s.mode, "listener");
        assert_eq!(s.address(), "srt://0.0.0.0:9000?mode=listener&latency=125");
        assert!(s.problem().is_none());
    }

    #[test]
    fn host_and_port_build_the_address() {
        let s = Settings::from_params(&json!({"mode": "caller", "host": "10.0.0.5",
                                              "port": 4200, "latency_ms": 200}));
        assert_eq!(s.address(), "srt://10.0.0.5:4200?mode=caller&latency=200");
    }

    #[test]
    fn a_pasted_uri_keeps_every_option_it_already_carries() {
        let s = Settings::from_params(&json!({
            "uri": "srt://h:9000?mode=caller&latency=400&pbkeylen=32",
            "mode": "listener", "latency_ms": 125}));
        let address = s.address();
        assert!(address.contains("mode=caller"), "{address}");
        assert!(address.contains("latency=400"), "{address}");
        assert!(!address.contains("latency=125"), "{address}");
        assert!(address.contains("pbkeylen=32"), "{address}");
    }

    #[test]
    fn a_stream_id_is_encoded_into_the_query() {
        let s = Settings::from_params(&json!({"host": "h", "port": 9000,
            "stream_id": "live/cam 1"}));
        assert!(s.address().contains("streamid=live%2Fcam%201"), "{}", s.address());
    }

    #[test]
    fn the_passphrase_never_reaches_the_address_at_all() {
        let s = Settings::from_params(&json!({"host": "h", "passphrase": "supersecret1"}));
        assert!(!s.address().contains("supersecret1"), "{}", s.address());
        assert!(!s.redacted().contains("supersecret1"), "{}", s.redacted());
    }

    #[test]
    fn a_passphrase_pasted_inside_a_uri_is_hidden_when_shown() {
        let s = Settings::from_params(&json!({"uri": "srt://h:9000?passphrase=supersecret1"}));
        let shown = s.redacted();
        assert!(!shown.contains("supersecret1"), "{shown}");
        assert!(shown.contains("passphrase=<set>"), "{shown}");
    }

    #[test]
    fn redact_leaves_an_address_with_no_query_alone() {
        assert_eq!(redact("srt://h:9000"), "srt://h:9000");
    }

    #[test]
    fn a_bad_scheme_is_named_with_the_way_out() {
        let s = Settings::from_params(&json!({"uri": "rtmp://h/live"}));
        let problem = s.problem().expect("rtmp is not srt");
        assert!(problem.contains("srt://"), "{problem}");
    }

    #[test]
    fn a_bad_mode_lists_the_three_that_work() {
        let s = Settings::from_params(&json!({"mode": "sideways"}));
        let problem = s.problem().expect("there is no sideways mode");
        assert!(problem.contains("rendezvous"), "{problem}");
    }

    #[test]
    fn a_short_passphrase_is_refused_before_srt_refuses_it() {
        let s = Settings::from_params(&json!({"host": "h", "passphrase": "short"}));
        let problem = s.problem().expect("five characters is under the floor");
        assert!(problem.contains("10 to 79"), "{problem}");
    }

    #[test]
    fn latency_is_held_inside_the_range_the_built_in_output_uses() {
        let s = Settings::from_params(&json!({"host": "h", "latency_ms": 99_999}));
        assert_eq!(s.latency_ms, 10_000);
    }

    #[test]
    fn configure_reads_the_whole_object_so_a_dropped_key_returns_to_its_default() {
        let first = Settings::from_params(&json!({"host": "h", "latency_ms": 900}));
        assert_eq!(first.latency_ms, 900);
        let second = Settings::from_params(&json!({"host": "h"}));
        assert_eq!(second.latency_ms, 125);
    }
}
