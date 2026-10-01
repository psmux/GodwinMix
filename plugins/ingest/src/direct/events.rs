//! What the host tells the station: `direct.input` and `direct.output` when
//! something moves, and `direct.stats` once a second, every show in one
//! event. One thread for the whole host, and only while it runs a show.
//!
//! Every number here is a counter or a reading the hub and the inputs keep
//! anyway; nothing is decoded or copied to make them. The events are built
//! under the shows' lock and sent after it is let go.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use godwinmix_protocol::destination::DestinationState;
use serde_json::{json, Map, Value};

use super::host::Host;
use super::show::Show;
use super::stats::show_stats;
use super::table::STREAM;

/// Where an event goes: the plugin's reporter, or a test.
pub type Emit = Arc<dyn Fn(&str, Value) + Send + Sync>;

const TICK: Duration = Duration::from_secs(1);

fn unix_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Start the watch thread unless it is running. It ends by itself when the
/// host has no show left, or is gone.
pub fn watch(host: &Arc<Host>) {
    if host.watching.swap(true, Ordering::AcqRel) {
        return;
    }
    let weak = Arc::downgrade(host);
    let started = std::thread::Builder::new().name("gmx-direct-watch".into()).spawn(move || {
        let mut said: HashMap<String, Value> = HashMap::new();
        loop {
            std::thread::sleep(TICK);
            let Some(host) = weak.upgrade() else { return };
            if host.lock().is_empty() {
                host.watching.store(false, Ordering::Release);
                return;
            }
            for (name, params) in tick(&host, &mut said) {
                (host.emit)(name, params);
            }
        }
    });
    if started.is_err() {
        host.watching.store(false, Ordering::Release);
    }
}

/// One second's events.
fn tick(host: &Host, said: &mut HashMap<String, Value>) -> Vec<(&'static str, Value)> {
    let shows = host.lock();
    let mut out = Vec::new();
    let mut rows = Vec::new();
    let mut seen_keys = Vec::new();
    for show in shows.values() {
        let id = &show.row.id;
        let (input, key) = input_event(host, show);
        seen_keys.push(format!("in:{id}"));
        if let Some(since) = moved(said, format!("in:{id}"), key) {
            out.push(("direct.input", with(input, "since_ms", json!(since))));
        }
        for o in &show.outputs {
            let live = o.live();
            let failed = (live.state == DestinationState::Failed).then(|| live.error.clone().unwrap_or_default());
            host.vitals.output(id, &o.wanted.id, failed.as_deref());
            let k = format!("out:{id}/{}", o.wanted.id);
            seen_keys.push(k.clone());
            if moved(said, k, json!([live.state, live.error, live.reconnects])).is_some() {
                let mut v = serde_json::to_value(&live).unwrap_or_else(|_| json!({}));
                v["show"] = json!(id);
                v["output"] = json!(o.wanted.id);
                out.push(("direct.output", v));
            }
        }
        let stats = show.seen.lock().unwrap_or_else(|e| e.into_inner()).stats.clone();
        host.vitals.counters(id, stats.cc_errors, stats.packets_lost);
        rows.push(show_stats(host, show));
    }
    drop(shows);
    said.retain(|k, _| seen_keys.contains(k));
    out.push(("direct.stats", json!({"shows": rows})));
    out
}

/// `Some(since)` when `key` differs from what was last said under `name`.
fn moved(said: &mut HashMap<String, Value>, name: String, key: Value) -> Option<u64> {
    if said.get(&name) == Some(&key) {
        return None;
    }
    said.insert(name, key);
    Some(unix_ms())
}

fn with(mut v: Value, k: &str, x: Value) -> Value {
    v[k] = x;
    v
}

/// `event/direct.input` for a show, and what decides whether it moved.
fn input_event(host: &Host, show: &Show) -> (Value, Value) {
    let app = show.row.app();
    let seen = show.seen.lock().unwrap_or_else(|e| e.into_inner());
    let live = seen.live();
    let error = seen.refused.clone().or_else(|| seen.stats.error.clone());
    // The inputs say so this way while the backup carries the show.
    let backup = seen.stats.error.as_deref().is_some_and(|e| e.starts_with("on the backup input"));
    drop(seen);
    let desc = host.hub.stream(&app, STREAM).unwrap_or(Value::Null);
    let mut v = Map::new();
    v.insert("show".into(), json!(show.row.id));
    v.insert("state".into(), json!(if live { "live" } else { "idle" }));
    v.insert("from".into(), json!(from(&show.row.input.uri)));
    v.insert("backup".into(), json!(backup));
    v.insert("relay".into(), json!((host.relay)()));
    v.insert("stream".into(), json!(format!("{app}/{STREAM}")));
    for k in ["video", "audio"] {
        if !desc[k].is_null() {
            v.insert(k.into(), desc[k].clone());
        }
    }
    if let Some(e) = &error {
        v.insert("error".into(), json!(e));
    }
    let shape = |k: &str| json!([desc[k]["codec"], desc[k]["width"], desc[k]["height"], desc[k]["channels"]]);
    // The relay too: it opens when a show first needs it, and a station
    // waiting to read the show learns where only from this event.
    let key = json!([live, backup, shape("video"), shape("audio"), error, (host.relay)()]);
    (Value::Object(v), key)
}

/// Where an input comes from, as a person would say it, with no password.
fn from(uri: &str) -> String {
    let Some((_, rest)) = uri.split_once("://") else { return uri.to_string() };
    let host = rest.split(['/', '?']).next().unwrap_or(rest);
    host.rsplit('@').next().unwrap_or(host).to_string()
}
