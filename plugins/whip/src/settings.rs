//! The settings of both provides, and the redaction that keeps the bearer
//! token out of every log line.
//!
//! WHIP and WHEP are the same idea pointed in opposite directions: one HTTP
//! POST carrying an SDP offer to an endpoint URL, with a bearer token if the
//! service wants one. So both provides read the same three or four keys and
//! this module is the whole of their configuration.

use serde_json::Value;

/// What `whip/output` and `whip/whep` are configured with.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// The WHIP or WHEP endpoint, an http:// or https:// URL.
    pub endpoint: String,
    /// Sent as `Authorization: Bearer <token>`. Empty for an open endpoint.
    pub token: String,
    /// `stun://host:port`, or empty for the element's own default.
    pub stun_server: String,
    /// `turn(s)://user:pass@host:port`, or empty for none.
    pub turn_server: String,
    /// Seconds to wait for the endpoint to answer. 0 is no timeout.
    pub timeout_secs: u32,
    /// Where the reconnect backoff starts, in milliseconds.
    pub reconnect_first_ms: u64,
    /// Where the reconnect backoff stops, in milliseconds.
    pub reconnect_max_ms: u64,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            endpoint: String::new(),
            token: String::new(),
            stun_server: String::new(),
            turn_server: String::new(),
            timeout_secs: 15,
            // Half a second, doubling to fifteen. The ceiling is lower than a
            // source's because an output that is down holds the core's outage
            // buffer open, and the buffer is finite.
            reconnect_first_ms: 500,
            reconnect_max_ms: 15_000,
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
    pub fn from_params(params: &Value) -> Settings {
        let d = Settings::default();
        Settings {
            endpoint: string(params, "endpoint").unwrap_or(d.endpoint),
            token: string(params, "token").unwrap_or(d.token),
            stun_server: string(params, "stun_server").unwrap_or(d.stun_server),
            turn_server: string(params, "turn_server").unwrap_or(d.turn_server),
            timeout_secs: params
                .get("timeout_secs")
                .and_then(Value::as_u64)
                .map(|v| v.min(300) as u32)
                .unwrap_or(d.timeout_secs),
            reconnect_first_ms: params
                .get("reconnect_first_ms")
                .and_then(Value::as_u64)
                .map(|v| v.clamp(100, 60_000))
                .unwrap_or(d.reconnect_first_ms),
            reconnect_max_ms: params
                .get("reconnect_max_ms")
                .and_then(Value::as_u64)
                .map(|v| v.clamp(100, 300_000))
                .unwrap_or(d.reconnect_max_ms),
        }
    }

    /// A value that is wrong in itself, whether or not the plugin is ready to
    /// connect. These are refused at `initialize` and at `configure`.
    ///
    /// A missing endpoint is deliberately not here. `configure` before `start`
    /// is legal and is how the first real params often arrive, so a plugin that
    /// died at `initialize` for want of a URL would refuse a conversation the
    /// protocol allows.
    pub fn malformed(&self) -> Option<String> {
        if self.endpoint.is_empty() {
            return None;
        }
        let lower = self.endpoint.to_lowercase();
        if !lower.starts_with("http://") && !lower.starts_with("https://") {
            return Some(format!(
                "endpoint is '{}'. WHIP and WHEP are HTTP: the URL begins http:// or https://.",
                self.endpoint
            ));
        }
        for (key, value) in [("stun_server", &self.stun_server)] {
            if !value.is_empty() && !value.to_lowercase().starts_with("stun://") {
                return Some(format!(
                    "{key} is '{value}'. Write it as stun://host:port, or leave it empty \
                     to use the element's default."
                ));
            }
        }
        if !self.turn_server.is_empty() {
            let lower = self.turn_server.to_lowercase();
            if !lower.starts_with("turn://") && !lower.starts_with("turns://") {
                return Some(format!(
                    "turn_server is '{}'. Write it as turn://user:password@host:port \
                     (or turns:// for TLS), or leave it empty.",
                    self.turn_server
                ));
            }
        }
        None
    }

    /// What stops this from connecting now. Refused at `start`.
    pub fn problem(&self) -> Option<String> {
        if self.endpoint.is_empty() {
            return Some(
                "there is no endpoint. Set 'endpoint' to the URL the service gave you, \
                 which looks like https://example.com/whip/<stream>."
                    .into(),
            );
        }
        self.malformed()
    }

    /// The endpoint as it is safe to print: a token in the query string, which
    /// some services still hand out, is hidden.
    pub fn redacted_endpoint(&self) -> String {
        redact(&self.endpoint)
    }

    /// Whether a token is set, which is all a client is ever told about it.
    pub fn has_token(&self) -> bool {
        !self.token.is_empty()
    }
}

/// Hide the value of any query key that looks like a credential.
pub fn redact(url: &str) -> String {
    let Some((head, query)) = url.split_once('?') else {
        return url.to_string();
    };
    let secret = |k: &str| {
        let k = k.to_lowercase();
        k.contains("token") || k.contains("key") || k.contains("secret") || k.contains("password")
    };
    let parts: Vec<String> = query
        .split('&')
        .map(|p| match p.split_once('=') {
            Some((k, _)) if secret(k) => format!("{k}=<set>"),
            _ => p.to_string(),
        })
        .collect();
    format!("{head}?{}", parts.join("&"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_endpoint_is_required_before_start_and_the_message_shows_the_shape() {
        let problem = Settings::default().problem().expect("no endpoint is a problem");
        assert!(problem.contains("https://"), "{problem}");
    }

    #[test]
    fn an_empty_endpoint_is_not_malformed_because_configure_may_still_supply_one() {
        assert!(Settings::default().malformed().is_none());
    }

    #[test]
    fn a_non_http_endpoint_is_refused() {
        let s = Settings::from_params(&json!({"endpoint": "srt://h:9000"}));
        let problem = s.malformed().expect("srt is not a WHIP endpoint");
        assert!(problem.contains("http://"), "{problem}");
    }

    #[test]
    fn a_good_endpoint_with_a_token_passes_and_the_token_is_never_the_endpoint() {
        let s = Settings::from_params(&json!({
            "endpoint": "https://example.com/whip/studio", "token": "bearer-secret"
        }));
        assert!(s.problem().is_none());
        assert!(s.has_token());
        assert!(!s.redacted_endpoint().contains("bearer-secret"));
    }

    #[test]
    fn a_token_in_the_query_string_is_hidden_when_the_endpoint_is_shown() {
        let s = Settings::from_params(&json!({
            "endpoint": "https://example.com/whip?auth_token=secret123&room=studio"
        }));
        let shown = s.redacted_endpoint();
        assert!(!shown.contains("secret123"), "{shown}");
        assert!(shown.contains("room=studio"), "{shown}");
    }

    #[test]
    fn a_stun_address_with_the_wrong_scheme_is_named() {
        let s = Settings::from_params(&json!({
            "endpoint": "https://e/whip", "stun_server": "example.com:3478"
        }));
        assert!(s.problem().expect("stun:// is required").contains("stun://"));
    }

    #[test]
    fn a_turn_address_may_be_turns() {
        let s = Settings::from_params(&json!({
            "endpoint": "https://e/whip", "turn_server": "turns://u:p@example.com:5349"
        }));
        assert!(s.problem().is_none());
    }

    #[test]
    fn the_backoff_bounds_are_held_inside_something_sensible() {
        let s = Settings::from_params(&json!({
            "endpoint": "https://e/whip", "reconnect_first_ms": 1, "reconnect_max_ms": 9_000_000
        }));
        assert_eq!(s.reconnect_first_ms, 100);
        assert_eq!(s.reconnect_max_ms, 300_000);
    }

    #[test]
    fn a_url_with_no_query_is_left_alone() {
        assert_eq!(redact("https://e/whip"), "https://e/whip");
    }
}
