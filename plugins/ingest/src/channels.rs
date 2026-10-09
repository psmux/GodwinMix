//! Who may publish where: the channel table the core hands this plugin.
//!
//! The core owns channels. It keeps them, persists them, seals their keys and
//! hands this plugin the table in its settings under `channels`, at
//! `initialize` and again on every change through `configure`. This module is
//! the one question the listener asks of it: may this publisher, asking for
//! this application and this stream name, come in, and if so as which stream
//! and on which key.
//!
//! A key may ride as `?psk=`, `?key=`, `?token=` or `?Token=` on the stream
//! name (or, for encoders that put it there, on the application name), or be
//! the whole stream name on a channel whose `key_mode` is `stream`. A stream
//! let in on the whole-name form is named after its key's id, so the key never
//! becomes part of a source id or a log line.

use serde_json::Value;

use crate::unescape;
pub use crate::proto::{Protocol, Tls};

/// The query parameters a key may arrive in, in the order they are looked for.
const KEY_PARAMS: [&str; 4] = ["psk", "key", "token", "Token"];

/// One channel, as far as the listener needs to know it.
#[derive(Debug, Clone, PartialEq)]
pub struct Channel {
    pub id: String,
    pub app: String,
    pub enabled: bool,
    /// The stream name is the key.
    pub key_in_name: bool,
    /// `(id, secret)`.
    pub keys: Vec<(String, String)>,
    /// What it takes publishers over. RTMPS is `rtmps_port`, not in here.
    pub protocols: Vec<Protocol>,
    /// RTMPS, on this port, when a person has turned it on.
    pub rtmps_port: Option<u16>,
}

impl Channel {
    /// Does this channel take publishers over `protocol`?
    pub fn takes(&self, protocol: Protocol) -> bool {
        match protocol {
            Protocol::Rtmps => self.rtmps_port.is_some(),
            other => self.protocols.contains(&other),
        }
    }
}

/// A publisher let in.
#[derive(Debug, Clone, PartialEq)]
pub struct Admit {
    pub channel: String,
    pub app: String,
    pub stream: String,
    pub key: String,
}

/// A publisher turned away, in the shape of `event/channel.refused`.
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    pub channel: String,
    /// Empty when the name itself was the key that did not match.
    pub stream: String,
    pub why: String,
}

/// Every channel. Empty means the core has none, and the listener takes any
/// publisher the way `ingest/discover` always has.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Table {
    pub channels: Vec<Channel>,
    /// The certificate RTMPS answers with, when there is one.
    pub tls: Option<Tls>,
}

impl Table {
    /// Read the `channels` member of the plugin's settings. Anything that is
    /// not a well formed channel is skipped rather than failing the rest.
    pub fn from_params(params: &Value) -> Table {
        let list = params.get("channels").and_then(Value::as_array).cloned().unwrap_or_default();
        let channels = list.iter().filter_map(channel_of).collect();
        Table { channels, tls: Tls::from_params(params) }
    }

    pub fn is_open(&self) -> bool {
        self.channels.is_empty()
    }

    /// Decide on one RTMP publisher. The tests ask it this way.
    #[cfg(test)]
    pub fn admit(&self, app_raw: &str, stream_raw: &str) -> Result<Admit, Refusal> {
        self.admit_via(Protocol::Rtmp, app_raw, stream_raw)
    }

    /// Decide on one publisher arriving over `protocol`.
    pub fn admit_via(&self, protocol: Protocol, app_raw: &str, stream_raw: &str) -> Result<Admit, Refusal> {
        let (app_sent, app_query) = split_query(app_raw);
        let app = &unescape::percent(app_sent);
        let (name, stream_query) = split_query(stream_raw);
        let refuse = |stream: &str, why: String| Refusal {
            channel: app.to_string(),
            stream: stream.to_string(),
            why,
        };
        let Some(channel) = self.find_app(app) else {
            return Err(refuse(name, format!(
                "there is no channel called '{app}' on this mixer. Check the server address \
                 in the encoder: it ends with the channel's name."
            )));
        };
        let shown = if channel.key_in_name { "" } else { name };
        let refuse = |stream: &str, why: String| Refusal {
            channel: channel.id.clone(),
            stream: stream.to_string(),
            why,
        };
        if !channel.enabled {
            return Err(refuse(shown, format!(
                "the channel '{app}' is switched off. Switch it on in the mixer's Channels \
                 page and publish again."
            )));
        }
        if !channel.takes(protocol) {
            return Err(refuse(shown, protocol.not_taken(app)));
        }
        let offered = if channel.key_in_name {
            Some(unescape::percent(name))
        } else {
            key_param(stream_query).or_else(|| key_param(app_query))
        };
        let Some(offered) = offered.filter(|k| !k.is_empty()) else {
            return Err(refuse(shown, format!(
                "the channel '{app}' needs a key. Put it on the stream name as \
                 {name}?psk=<key>, or copy the whole stream key from the mixer's Channels page."
            )));
        };
        let Some((key, _)) = channel.keys.iter().find(|(_, secret)| same(secret, &offered)) else {
            return Err(refuse(shown, format!(
                "that key is not one of the keys of the channel '{app}'. It may have been \
                 taken back; copy the current one from the mixer's Channels page."
            )));
        };
        let stream = if channel.key_in_name { key.clone() } else { name.to_string() };
        if stream.is_empty() {
            return Err(refuse("", "the stream has no name. Publish to <server>/main?psk=<key>, \
                 where main can be any name you like."
                .to_string()));
        }
        // The channel's own spelling, whatever case the encoder used, so the
        // hub, the core and a source all name the stream one way.
        Ok(Admit { channel: channel.id.clone(), app: channel.app.clone(), stream, key: key.clone() })
    }
}

impl Table {
    /// The channel an encoder means by `app`: escapes undone, and without
    /// regard to case, so `Church`, `church` and `CHURCH` all arrive. The
    /// core refuses two channels whose names differ only in case, so this
    /// never has two to choose between.
    pub fn find_app(&self, app: &str) -> Option<&Channel> {
        let app = unescape::percent(app);
        self.channels.iter().find(|c| c.app.eq_ignore_ascii_case(&app))
    }

    /// Would a publisher on this channel, with this key, still be let in?
    /// Asked of everyone on air when the table changes.
    pub fn still_admits(&self, channel: &str, key: &str, protocol: Protocol) -> bool {
        self.channels.iter().any(|c| {
            c.id == channel && c.enabled && c.takes(protocol) && c.keys.iter().any(|(id, _)| id == key)
        })
    }
}

fn channel_of(value: &Value) -> Option<Channel> {
    let text = |k: &str| value.get(k).and_then(Value::as_str).map(str::to_string);
    let id = text("id")?;
    let keys = value
        .get("keys")
        .and_then(Value::as_array)
        .map(|ks| {
            ks.iter()
                .filter_map(|k| Some((k.get("id")?.as_str()?.to_string(), k.get("secret")?.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default();
    Some(Channel {
        app: text("app").unwrap_or_else(|| id.clone()),
        id,
        enabled: value.get("enabled").and_then(Value::as_bool).unwrap_or(true),
        key_in_name: text("key_mode").as_deref() == Some("stream"),
        keys,
        protocols: protocols_of(value),
        rtmps_port: value
            .get("rtmps_port")
            .and_then(Value::as_u64)
            .and_then(|p| u16::try_from(p).ok())
            .filter(|p| *p > 0),
    })
}

/// A table from a core that predates `protocols` meant RTMP, so no list is
/// RTMP alone.
fn protocols_of(value: &Value) -> Vec<Protocol> {
    match value.get("protocols").and_then(Value::as_array) {
        None => vec![Protocol::Rtmp],
        Some(list) => list.iter().filter_map(Value::as_str).filter_map(Protocol::parse).collect(),
    }
}

/// `name?a=b` into `name` and `a=b`.
pub fn split_query(raw: &str) -> (&str, &str) {
    let raw = raw.trim().trim_matches('/');
    match raw.split_once('?') {
        Some((name, query)) => (name, query),
        None => (raw, ""),
    }
}

pub fn key_param(query: &str) -> Option<String> {
    let pairs: Vec<(&str, &str)> =
        query.split('&').filter_map(|p| p.split_once('=')).collect();
    KEY_PARAMS
        .iter()
        .find_map(|want| pairs.iter().find(|(k, _)| k == want).map(|(_, v)| unescape::query_value(v)))
}

/// Compare two keys in time that does not depend on where they differ.
fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
#[path = "channels_tests.rs"]
mod tests;
