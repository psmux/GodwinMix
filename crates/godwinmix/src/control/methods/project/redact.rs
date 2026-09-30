//! Taking the secrets out of a source or an output before it leaves the mixer.
//!
//! The same rule `preset.save` uses for a stream address (the tail of an
//! RTMP or SRT path is the key), plus the query words and parameter names
//! that carry a password or a key. What was taken is written on the entry as
//! `$removed`, so an import knows the entry cannot start as it is and can say
//! what it is waiting for.

use serde_json::{Map, Value};

/// Where the list of what was taken out is kept on an entry.
pub const REMOVED: &str = "$removed";

/// Parameter and query names that hold a secret.
const SECRET_WORDS: &[&str] = &["key", "psk", "token", "passphrase", "password", "secret", "streamid", "auth"];

fn is_secret_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SECRET_WORDS.iter().any(|w| lower == *w || lower.ends_with(&format!("_{w}")) || lower.starts_with(&format!("{w}_")))
}

/// Take the secrets out of one entry. Answers what was taken, in words.
pub fn entry(value: &mut Value) -> Vec<String> {
    let Some(object) = value.as_object_mut() else { return Vec::new() };
    let mut taken = Vec::new();
    if let Some(Value::String(uri)) = object.get_mut("uri") {
        let (clean, what) = uri_without_secrets(uri);
        if !what.is_empty() {
            *uri = clean;
            taken.extend(what);
        }
    }
    if let Some(Value::Object(params)) = object.get_mut("params") {
        taken.extend(params_without_secrets(params, "params."));
    }
    let top: Vec<String> = object.keys().filter(|k| is_secret_name(k)).cloned().collect();
    for name in top {
        object.remove(&name);
        taken.push(name);
    }
    if !taken.is_empty() {
        object.insert(REMOVED.into(), Value::Array(taken.iter().cloned().map(Value::String).collect()));
    }
    taken
}

fn params_without_secrets(params: &mut Map<String, Value>, prefix: &str) -> Vec<String> {
    let names: Vec<String> = params.keys().filter(|k| is_secret_name(k)).cloned().collect();
    for name in &names {
        params.remove(name);
    }
    names.into_iter().map(|n| format!("{prefix}{n}")).collect()
}

/// An address with its password, its secret query words and its stream key
/// taken out, and what was taken.
pub fn uri_without_secrets(uri: &str) -> (String, Vec<String>) {
    let mut taken = Vec::new();
    let Some((scheme, rest)) = uri.split_once("://") else { return (uri.to_string(), taken) };
    let (rest, query) = match rest.split_once('?') {
        Some((r, q)) => (r.to_string(), Some(q.to_string())),
        None => (rest.to_string(), None),
    };
    let mut rest = rest;
    if let Some((userinfo, host)) = rest.split_once('@') {
        if userinfo.contains(':') {
            taken.push("the password in its address".to_string());
            rest = format!("{}@{host}", userinfo.split(':').next().unwrap_or(""));
        }
    }
    let streaming = matches!(scheme, "rtmp" | "rtmps");
    if streaming {
        if let Some((head, tail)) = rest.rsplit_once('/') {
            if head.contains('/') && tail.len() >= 6 {
                taken.push("its stream key".to_string());
                rest = head.to_string();
            }
        }
    }
    let query = query.map(|q| {
        let kept: Vec<&str> = q
            .split('&')
            .filter(|pair| {
                let name = pair.split('=').next().unwrap_or("");
                let secret = is_secret_name(name);
                if secret {
                    taken.push(format!("{name} in its address"));
                }
                !secret
            })
            .collect();
        kept.join("&")
    });
    let mut out = format!("{scheme}://{rest}");
    if let Some(q) = query.filter(|q| !q.is_empty()) {
        out.push('?');
        out.push_str(&q);
    }
    (out, taken)
}

/// What an entry lost on the way out, if anything.
pub fn removed_from(value: &Value) -> Vec<String> {
    value
        .get(REMOVED)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_stream_key_a_password_and_a_passphrase_come_out() {
        let (u, t) = uri_without_secrets("rtmp://a.rtmp.youtube.com/live2/abcd-efgh-ijkl");
        assert_eq!(u, "rtmp://a.rtmp.youtube.com/live2");
        assert_eq!(t, vec!["its stream key"]);
        let (u, t) = uri_without_secrets("srt://host:9000?mode=caller&passphrase=hunter22hunter");
        assert_eq!(u, "srt://host:9000?mode=caller");
        assert_eq!(t.len(), 1);
        let (u, _) = uri_without_secrets("rtsp://admin:pw@10.0.0.9/stream1");
        assert_eq!(u, "rtsp://admin@10.0.0.9/stream1");
        let (u, t) = uri_without_secrets("udp://239.0.0.1:5000");
        assert_eq!(u, "udp://239.0.0.1:5000");
        assert!(t.is_empty());
    }

    #[test]
    fn an_entry_says_what_it_lost_and_one_with_nothing_secret_is_untouched() {
        let mut out = json!({"id": "yt", "uri": "rtmp://x.example/live/secretkey99", "params": {"stream_key": "k", "latency": 200}});
        let taken = entry(&mut out);
        assert_eq!(taken.len(), 2, "{taken:?}");
        assert_eq!(out["params"], json!({"latency": 200}));
        assert_eq!(removed_from(&out).len(), 2);
        let mut plain = json!({"id": "cam1", "uri": "test://smpte"});
        assert!(entry(&mut plain).is_empty());
        assert!(plain.get(REMOVED).is_none());
    }
}
