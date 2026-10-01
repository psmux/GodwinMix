//! `InputSpec`: what a direct show takes, as the contract writes it, and the
//! kind of input each address means.

use serde_json::{json, Value};

/// Every way in a direct show has, by the scheme of its address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `udp://` or `rtp://`: MPEG-TS, bare or in RTP, unicast or multicast.
    Udp,
    Srt,
    Rtsp,
    Rist,
    /// `http(s)://` to a playlist or a manifest: HLS or DASH.
    Http,
    Rtmp,
    /// A file on this machine, looped.
    File,
    /// `channel:<app>/<stream>`, read straight off the hub.
    Channel,
}

pub const SCHEMES: &[&str] =
    &["udp://", "rtp://", "srt://", "rtsp://", "rtsps://", "rist://", "http://", "https://", "rtmp://", "rtmps://", "file://", "channel:"];

/// What a direct show takes.
#[derive(Debug, Clone, PartialEq)]
pub struct InputSpec {
    pub uri: String,
    /// The MPEG-TS program to take from a multiplex; `None` takes the first.
    pub program: Option<u16>,
    /// Per transport: `interface` and `source_address` for multicast,
    /// `latency_ms`, `passphrase` and `mode` for SRT, `transport` for RTSP.
    pub params: Value,
    pub backup: Option<Box<InputSpec>>,
}

/// A refusal: the sentence and a `data` object a caller can act on.
#[derive(Debug, Clone, PartialEq)]
pub struct InputError {
    pub message: String,
    pub data: Value,
}

impl InputError {
    pub fn new(message: impl Into<String>, data: Value) -> InputError {
        InputError { message: message.into(), data }
    }
}

impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl InputSpec {
    pub fn new(uri: &str) -> InputSpec {
        InputSpec { uri: uri.to_string(), program: None, params: json!({}), backup: None }
    }

    /// Read the contract's JSON. A bare string is taken as the address.
    pub fn from_json(v: &Value) -> Result<InputSpec, InputError> {
        if let Some(uri) = v.as_str() {
            let spec = InputSpec::new(uri.trim());
            spec.kind()?;
            return Ok(spec);
        }
        let uri = v.get("uri").and_then(Value::as_str).map(str::trim).unwrap_or_default();
        let program = match v.get("program") {
            None | Some(Value::Null) => None,
            Some(p) => Some(p.as_u64().and_then(|n| u16::try_from(n).ok()).filter(|&n| n > 0).ok_or_else(|| {
                InputError::new(
                    format!("program {p} is not an MPEG-TS program number. Give a number from 1 to 65535, as the feed's programs list shows, or leave it out to take the first."),
                    json!({"field": "program", "got": p}),
                )
            })?),
        };
        let params = v.get("params").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
        let backup = match v.get("backup") {
            None | Some(Value::Null) => None,
            Some(b) => Some(Box::new(InputSpec::from_json(b)?)),
        };
        let spec = InputSpec { uri: uri.to_string(), program, params, backup };
        spec.kind()?;
        Ok(spec)
    }

    pub fn json(&self) -> Value {
        let mut v = json!({"uri": self.uri});
        if let Some(p) = self.program {
            v["program"] = json!(p);
        }
        if self.params.as_object().is_some_and(|o| !o.is_empty()) {
            v["params"] = self.params.clone();
        }
        if let Some(b) = &self.backup {
            v["backup"] = b.json();
        }
        v
    }

    /// Which input this address means, or why none does.
    pub fn kind(&self) -> Result<Kind, InputError> {
        let lower = self.uri.to_ascii_lowercase();
        let kind = match lower.split_once("://").map(|(s, _)| s) {
            Some("udp" | "rtp") => Kind::Udp,
            Some("srt") => Kind::Srt,
            Some("rtsp" | "rtsps" | "rtspt") => Kind::Rtsp,
            Some("rist") => Kind::Rist,
            Some("http" | "https") => Kind::Http,
            Some("rtmp" | "rtmps") => Kind::Rtmp,
            Some("file") => Kind::File,
            _ if lower.starts_with("channel:") => Kind::Channel,
            _ if self.uri.starts_with('/') => Kind::File,
            _ => {
                return Err(InputError::new(
                    format!(
                        "'{}' is not an address a direct show can take. Write one of udp://@239.1.1.1:5000, \
                         srt://host:9000, rtsp://camera/stream, https://host/live.m3u8, rtmp://host/app/key, \
                         rist://@0.0.0.0:5004, file:///clip.ts or channel:<app>/<stream>.",
                        self.uri
                    ),
                    json!({"field": "uri", "got": self.uri, "schemes": SCHEMES}),
                ))
            }
        };
        Ok(kind)
    }

    /// A string param, trimmed, when it is set.
    pub fn param(&self, key: &str) -> Option<String> {
        self.params.get(key).and_then(Value::as_str).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    }

    /// A number param, when it is set.
    pub fn number(&self, key: &str) -> Option<u64> {
        self.params.get(key).and_then(Value::as_u64)
    }
}

#[cfg(test)]
#[path = "spec_tests.rs"]
mod tests;
