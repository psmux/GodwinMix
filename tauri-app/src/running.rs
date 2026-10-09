//! What the mixer is running, in words, read from the same public API the
//! page uses: `output.list` and `channel.list`.
//!
//! The window closing used to hide it and say nothing, so a stream to
//! YouTube carried on with no window and no word about it. The shell now asks
//! before it leaves anything running, and this is what it asks about. The
//! page has its own copy of these rules in `ui/panels/header/running-model.js`
//! and the two are kept to the same words.

use serde_json::Value;

/// One thing running, as a line a person reads and the call that stops it.
#[derive(Debug, Clone, PartialEq)]
pub struct Thing {
    pub line: String,
    /// Sent somewhere: a stream, a watch link, a recording. An encoder
    /// sending in to a channel is running but is not outgoing.
    pub outgoing: bool,
    pub stop: Option<Stop>,
}

/// A REST call that stops one thing.
#[derive(Debug, Clone, PartialEq)]
pub struct Stop {
    pub method: &'static str,
    pub path: String,
    pub body: Option<Value>,
}

/// "YouTube" for an address at youtube.com, else the destination's own name.
pub fn platform(host: &str, fallback: &str) -> String {
    let host = host.to_lowercase();
    let known = [
        ("youtube", "YouTube"),
        ("facebook", "Facebook"),
        ("twitch", "Twitch"),
        ("live-video.net", "Twitch"),
        ("kick", "Kick"),
        ("linkedin", "LinkedIn"),
        ("vimeo", "Vimeo"),
        ("restream", "Restream"),
    ];
    known.iter().find(|(k, _)| host.contains(k)).map_or_else(|| fallback.to_string(), |(_, name)| name.to_string())
}

/// `1:56:23`, or `4:05` under an hour, the way the page writes it.
pub fn clock(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn secs(v: &Value, key: &str) -> Option<u64> {
    v.get(key).and_then(Value::as_u64)
}

/// One programme output, or nothing when it is stopped or never dialled.
fn output(o: &Value) -> Option<Thing> {
    let (id, state) = (text(o, "id"), text(o, "state"));
    let live = state == "live";
    let dialling = matches!(state, "connecting" | "reconnecting") && o.get("has_key") != Some(&Value::Bool(false));
    let stop = |method, path: String| Some(Stop { method, path, body: None });
    match text(o, "type") {
        _ if state == "stopped" => None,
        "record/output" => Some(Thing {
            line: secs(o, "recording_secs").filter(|_| live).map_or("Recording".into(), |s| format!("Recording, for {}", clock(s))),
            outgoing: true,
            stop: stop("DELETE", format!("/api/v1/outputs/{id}")),
        }),
        "hls/output" => Some(Thing { line: format!("Serving the watch link {id}"), outgoing: true, stop: stop("POST", format!("/api/v1/outputs/{id}/stop")) }),
        _ if !live && !dialling => None,
        _ => {
            let name = platform(text(o, "uri_host"), id);
            let how = match secs(o, "live_secs") {
                Some(s) if live => format!(", live for {}", clock(s)),
                _ if live => ", live".into(),
                _ => ", connecting".into(),
            };
            Some(Thing { line: format!("Streaming to {name}{how}"), outgoing: true, stop: stop("POST", format!("/api/v1/outputs/{id}/stop")) })
        }
    }
}

/// A channel's destinations that are on and sending, and its live encoders.
fn channel(c: &Value) -> Vec<Thing> {
    let (id, name) = (text(c, "id"), text(c, "name"));
    let name = if name.is_empty() { id } else { name };
    let mut things = Vec::new();
    for d in c.get("destinations").and_then(Value::as_array).into_iter().flatten() {
        let on = d.get("enabled").and_then(Value::as_bool).unwrap_or(false);
        if !on || !matches!(text(d, "state"), "live" | "connecting" | "reconnecting") {
            continue;
        }
        let label = if text(d, "label").is_empty() { text(d, "id") } else { text(d, "label") };
        things.push(Thing {
            line: format!("Channel {name} sending on to {label}"),
            outgoing: true,
            stop: Some(Stop {
                method: "POST",
                path: format!("/api/v1/channels/{id}/destination"),
                body: Some(serde_json::json!({ "destination": text(d, "id"), "enabled": false })),
            }),
        });
    }
    let streams = c.get("streams").and_then(Value::as_array).into_iter().flatten();
    let live: Vec<&Value> = streams.filter(|s| text(s, "state") == "live").collect();
    if !live.is_empty() && c.get("enabled") != Some(&Value::Bool(false)) {
        let from = text(live[0], "from");
        let line = match (live.len(), from.is_empty()) {
            (1, false) => format!("Channel {name} receiving from {from}"),
            (1, true) => format!("Channel {name} receiving a stream"),
            (n, _) => format!("Channel {name} receiving {n} streams"),
        };
        things.push(Thing { line, outgoing: false, stop: None });
    }
    things
}

/// Everything running, programme first. `outputs` is what `output.list`
/// answered and `channels` what `channel.list` did, either of them `Null`
/// when it could not be read.
pub fn describe(outputs: &Value, channels: &Value) -> Vec<Thing> {
    let outputs = outputs.as_array().or_else(|| outputs.get("outputs").and_then(Value::as_array));
    let mut things: Vec<Thing> = outputs.into_iter().flatten().filter_map(output).collect();
    let channels = channels.get("channels").and_then(Value::as_array);
    things.extend(channels.into_iter().flatten().flat_map(channel));
    things
}

#[cfg(test)]
mod tests;
