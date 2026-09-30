//! What an SRT caller asked for, read from its stream id.
//!
//! Two spellings are taken, both common in encoders:
//!
//! * `<channel>/<stream>`, with the key on it as a query if the caller does
//!   not use a passphrase: `sunday-service/main?psk=<key>`.
//! * The SRT access control syntax, `#!::r=<channel>/<stream>,m=publish`,
//!   where `u=<key id>` names which of the channel's keys is the passphrase
//!   and `psk=`, `key=` or `token=` may carry the key itself.
//!
//! A stream id with no stream in it (`sunday-service`) is the stream `main`.

use crate::channels::{key_param, split_query};

/// A caller's request, in the terms the channel table asks about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub app: String,
    /// The stream name, with `?psk=<key>` on it when the key came in the id.
    pub stream: String,
    /// `u=` from the access control syntax: the key the passphrase is.
    pub user: Option<String>,
    /// `m=request` is a player, which this port sends the stream to.
    pub publish: bool,
}

impl Route {
    /// Did the key come in the stream id, rather than as a passphrase?
    pub fn has_key(&self) -> bool {
        key_param(split_query(&self.stream).1).is_some()
    }

    /// The stream name alone, without any key.
    pub fn name(&self) -> &str {
        split_query(&self.stream).0
    }
}

/// Read a stream id. `None` for one that names no channel at all.
pub fn parse(raw: &str) -> Option<Route> {
    let raw = raw.trim();
    match raw.strip_prefix("#!::") {
        Some(pairs) => access_control(pairs),
        None => plain(raw, None, true),
    }
}

fn plain(resource: &str, user: Option<String>, publish: bool) -> Option<Route> {
    let (path, query) = split_query(resource);
    let (app, stream) = match path.split_once('/') {
        Some((app, stream)) => (app, if stream.is_empty() { "main" } else { stream }),
        None => (path, "main"),
    };
    if app.is_empty() {
        return None;
    }
    let stream = if query.is_empty() { stream.to_string() } else { format!("{stream}?{query}") };
    Some(Route { app: app.to_string(), stream, user, publish })
}

fn access_control(pairs: &str) -> Option<Route> {
    let mut resource = None;
    let (mut user, mut publish, mut key) = (None, true, None);
    for pair in pairs.split(',') {
        let Some((k, v)) = pair.split_once('=') else { continue };
        match k.trim() {
            "r" => resource = Some(v.trim().to_string()),
            "u" => user = Some(v.trim().to_string()).filter(|u| !u.is_empty()),
            "m" => publish = v.trim() != "request",
            "psk" | "key" | "token" | "Token" => key = Some(v.trim().to_string()),
            _ => {}
        }
    }
    let mut route = plain(&resource?, user, publish)?;
    if let Some(key) = key.filter(|k| !k.is_empty()) {
        let joiner = if route.stream.contains('?') { '&' } else { '?' };
        route.stream = format!("{}{joiner}psk={key}", route.stream);
    }
    Some(route)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_id_is_channel_and_stream() {
        let r = parse("sunday-service/cam2").unwrap();
        assert_eq!((r.app.as_str(), r.stream.as_str(), r.publish), ("sunday-service", "cam2", true));
        assert!(!r.has_key());
        assert_eq!(parse("sunday-service").unwrap().stream, "main");
        assert!(parse("").is_none());
    }

    #[test]
    fn a_key_in_the_id_is_kept_on_the_stream_for_the_table() {
        let r = parse("church/main?psk=abc").unwrap();
        assert_eq!(r.stream, "main?psk=abc");
        assert!(r.has_key());
        assert_eq!(r.name(), "main");
    }

    #[test]
    fn the_access_control_syntax_is_read_whole() {
        let r = parse("#!::r=church/cam2,m=publish,u=obs-laptop").unwrap();
        assert_eq!((r.app.as_str(), r.stream.as_str()), ("church", "cam2"));
        assert_eq!(r.user.as_deref(), Some("obs-laptop"));
        let keyed = parse("#!::m=publish,r=church/cam2,psk=abc").unwrap();
        assert_eq!(keyed.stream, "cam2?psk=abc");
        assert!(!parse("#!::r=church/main,m=request").unwrap().publish);
        assert!(parse("#!::m=publish").is_none(), "no resource names no channel");
    }
}
