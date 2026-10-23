//! `event/direct.input`: one show's input as the station hears of it, and
//! what decides whether it moved enough to say again.

use serde_json::{json, Map, Value};

use super::host::Host;
use super::show::Show;
use super::table::STREAM;

/// `event/direct.input` for a show, and what decides whether it moved.
pub fn input_event(host: &Host, show: &Show) -> (Value, Value) {
    let app = show.row.app();
    let seen = show.seen.lock().unwrap_or_else(|e| e.into_inner());
    let live = seen.live();
    let error = seen.refused.clone().or_else(|| seen.stats.error.clone());
    // The inputs say so this way while the backup carries the show.
    let backup = seen.stats.error.as_deref().is_some_and(|e| e.starts_with("on the backup input"));
    let pmt = seen.stats.tracks.clone();
    let scrambled = seen.stats.scrambled;
    drop(seen);
    let desc = host.hub.stream(&app, STREAM).unwrap_or(Value::Null);
    let mut v = Map::new();
    v.insert("show".into(), json!(show.row.id));
    v.insert("state".into(), json!(if live { "live" } else { "idle" }));
    v.insert("from".into(), json!(from(&show.row.input.uri)));
    v.insert("backup".into(), json!(backup));
    v.insert("scrambled".into(), json!(scrambled));
    v.insert("relay".into(), json!((host.relay)()));
    v.insert("stream".into(), json!(format!("{app}/{STREAM}")));
    for k in ["video", "audio"] {
        if !desc[k].is_null() {
            v.insert(k.into(), desc[k].clone());
        }
    }
    if let Some(video) = v.get_mut("video").filter(|_| pmt.iter().any(|t| t.kind == "video" && t.field_coded)) {
        super::stats::fields_to_frames(video);
    }
    let tracks = super::stats::tracks(&desc["tracks"], &pmt);
    if tracks.as_array().is_some_and(|t| !t.is_empty()) {
        v.insert("tracks".into(), tracks.clone());
    }
    if let Some(e) = &error {
        v.insert("error".into(), json!(e));
    }
    let lineup: Vec<Value> = tracks.as_array().into_iter().flatten().map(|t| json!([t["track"], t["kind"], t["codec"], t["channels"], t["lang"]])).collect();
    let shape = |k: &str| json!([desc[k]["codec"], desc[k]["width"], desc[k]["height"], desc[k]["channels"]]);
    // The relay too: it opens when a show first needs it, and a station
    // waiting to read the show learns where only from this event.
    let key = json!([live, backup, shape("video"), shape("audio"), error, (host.relay)(), lineup, scrambled > 0]);
    (Value::Object(v), key)
}

/// Where an input comes from, as a person would say it, with no password.
fn from(uri: &str) -> String {
    let Some((_, rest)) = uri.split_once("://") else { return uri.to_string() };
    let host = rest.split(['/', '?']).next().unwrap_or(rest);
    host.rsplit('@').next().unwrap_or(host).to_string()
}
