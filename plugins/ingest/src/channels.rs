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
}

impl Table {
    /// Read the `channels` member of the plugin's settings. Anything that is
    /// not a well formed channel is skipped rather than failing the rest.
    pub fn from_params(params: &Value) -> Table {
        let list = params.get("channels").and_then(Value::as_array).cloned().unwrap_or_default();
        let channels = list.iter().filter_map(channel_of).collect();
        Table { channels }
    }

    pub fn is_open(&self) -> bool {
        self.channels.is_empty()
    }

    /// Decide on one publisher.
    pub fn admit(&self, app_raw: &str, stream_raw: &str) -> Result<Admit, Refusal> {
        let (app, app_query) = split_query(app_raw);
        let (name, stream_query) = split_query(stream_raw);
        let refuse = |stream: &str, why: String| Refusal {
            channel: app.to_string(),
            stream: stream.to_string(),
            why,
        };
        let Some(channel) = self.channels.iter().find(|c| c.app == app) else {
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
        let offered = if channel.key_in_name {
            Some(name.to_string())
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
        Ok(Admit { channel: channel.id.clone(), app: app.to_string(), stream, key: key.clone() })
    }
}

impl Table {
    /// Would a publisher on this channel, with this key, still be let in?
    /// Asked of everyone on air when the table changes.
    pub fn still_admits(&self, channel: &str, key: &str) -> bool {
        self.channels
            .iter()
            .any(|c| c.id == channel && c.enabled && c.keys.iter().any(|(id, _)| id == key))
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
    })
}

/// `name?a=b` into `name` and `a=b`.
pub fn split_query(raw: &str) -> (&str, &str) {
    let raw = raw.trim().trim_matches('/');
    match raw.split_once('?') {
        Some((name, query)) => (name, query),
        None => (raw, ""),
    }
}

fn key_param(query: &str) -> Option<String> {
    let pairs: Vec<(&str, &str)> =
        query.split('&').filter_map(|p| p.split_once('=')).collect();
    KEY_PARAMS
        .iter()
        .find_map(|want| pairs.iter().find(|(k, _)| k == want).map(|(_, v)| v.to_string()))
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
