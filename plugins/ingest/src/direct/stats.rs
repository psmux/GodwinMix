//! One show's row in `direct.stats`: what the input said, with what the hub
//! measured filling the blanks, and each output's state and rate.

use serde_json::{json, Value};

use super::host::Host;
use super::show::Show;
use super::table::STREAM;

/// One show's row in `direct.stats`: the input's numbers, with what the hub
/// measured filling in what the input left blank, and each output's.
pub fn show_stats(host: &Host, show: &Show) -> Value {
    let seen = show.seen.lock().unwrap_or_else(|e| e.into_inner());
    let mut input = seen.stats.json();
    let last_tag = seen.last_tag.map(|t| t.elapsed().as_millis() as u64);
    drop(seen);
    let desc = host.hub.stream(&show.row.app(), STREAM).unwrap_or(Value::Null);
    fill(&mut input, &desc, last_tag);
    let outputs: Vec<Value> = show
        .outputs
        .iter()
        .map(|o| {
            let live = o.live();
            let mut v = json!({"id": o.wanted.id, "state": live.state, "kbps": live.kbps, "reconnects": live.reconnects});
            if let Some(e) = &o.encoder {
                v["encoder"] = json!(e);
            }
            v
        })
        .collect();
    json!({"id": show.row.id, "input": input, "outputs": outputs, "dropped_gops": desc["dropped_gops"].as_u64().unwrap_or(0)})
}

/// Fill the blanks an input left from the hub's own meter, and leave out
/// what nobody can tell yet.
fn fill(input: &mut Value, desc: &Value, last_tag: Option<u64>) {
    let (v, a) = (&desc["video"], &desc["audio"]);
    let blank = |x: &Value| x.is_null() || x == &json!(0) || x == &json!(0.0) || x == &json!("");
    let kbps = v["kbps"].as_u64().unwrap_or(0) + a["kbps"].as_u64().unwrap_or(0);
    for (k, from) in [
        ("kbps", json!(kbps)),
        ("fps", v["fps"].clone()),
        ("width", v["width"].clone()),
        ("height", v["height"].clone()),
        ("video_codec", v["codec"].clone()),
        ("audio_codec", a["codec"].clone()),
        ("audio_channels", a["channels"].clone()),
        ("last_frame_ms", last_tag.map_or(Value::Null, |m| json!(m))),
    ] {
        if blank(&input[k]) && !blank(&from) {
            input[k] = from;
        }
    }
    if let Some(o) = input.as_object_mut() {
        o.retain(|_, x| !x.is_null());
    }
}
