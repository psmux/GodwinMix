//! What the direct host says, kept and acted on.
//!
//! The plugin pump hands every `direct.*` event to a queue and goes back to
//! its other plugins; this module's own thread takes them off it. An input
//! that goes live or changes shape is planned again when the show converts
//! something; a show that composites gets its input as a source; every
//! change that moves a show's health is announced.

use super::seen::Seen;
use crate::station::state::Station;
use godwinmix_core::plugin::supervisor::Supervisor;
use serde_json::Value;
use std::sync::{Arc, Weak};
use tracing::{debug, warn};

pub fn start(st: &Arc<Station>, plugins: &Arc<Supervisor>) {
    let (tx, rx) = std::sync::mpsc::channel::<(String, Value)>();
    plugins.route_events(
        "direct.",
        Arc::new(move |_, name, params| {
            let _ = tx.send((name.to_string(), params.clone()));
        }),
    );
    let weak: Weak<Station> = Arc::downgrade(st);
    let runtime = tokio::runtime::Handle::current();
    let spawned = std::thread::Builder::new().name("direct-events".into()).spawn(move || {
        while let Ok((name, params)) = rx.recv() {
            let Some(st) = weak.upgrade() else { return };
            take(&st, &name, &params);
            if name == "direct.input" {
                let id = params["show"].as_str().unwrap_or_default().to_string();
                runtime.block_on(super::view::feed_source(&st, &id));
            }
        }
    });
    if let Err(e) = spawned {
        warn!(?e, "no thread for direct host events; show health will not move");
    }
}

/// One event from the host. Public to the station's tests, which play the
/// host's part.
pub fn take(st: &Arc<Station>, name: &str, v: &Value) {
    match name {
        "direct.input" => input(st, v),
        "direct.output" => output(st, v),
        "direct.health" => host_health(st, v),
        "direct.stats" => stats(st, v),
        other => debug!(event = other, "a direct host event nothing acts on"),
    }
}

fn known(st: &Station, id: &str) -> bool {
    st.registry.lock().get(id).is_some()
}

fn with_seen<T>(st: &Station, id: &str, f: impl FnOnce(&mut Seen) -> T) -> T {
    f(st.direct.seen.lock().entry(id.to_string()).or_default())
}

fn input(st: &Arc<Station>, v: &Value) {
    let id = v["show"].as_str().unwrap_or_default().to_string();
    if !known(st, &id) {
        return;
    }
    let shape = |x: &Value| (x["state"].clone(), x["video"].clone(), x["audio"].clone(), x["relay"].clone(), x["stream"].clone());
    let moved = with_seen(st, &id, |s| {
        let moved = s.input.as_ref().map(shape) != Some(shape(v));
        s.input = Some(v.clone());
        moved
    });
    if moved && converts(st, &id) {
        st.direct.hand_over();
    }
    st.announce(&id);
    st.direct.announce_health(st, &id);
}

/// Whether any output of the show asks for a rendition.
fn converts(st: &Station, id: &str) -> bool {
    st.registry.lock().get(id).is_some_and(|r| !r.compositing && r.outputs.iter().any(|o| o.rendition.is_some() && o.enabled))
}

fn output(st: &Arc<Station>, v: &Value) {
    let id = v["show"].as_str().unwrap_or_default().to_string();
    let out = v["output"].as_str().unwrap_or_default().to_string();
    let on = st.registry.lock().get(&id).is_some_and(|r| r.outputs.iter().any(|o| o.id == out && o.enabled));
    if !on {
        return;
    }
    if with_seen(st, &id, |s| s.take_output(&out, v)) {
        st.announce(&id);
        st.direct.announce_health(st, &id);
    }
}

fn host_health(st: &Arc<Station>, v: &Value) {
    let id = v["show"].as_str().unwrap_or_default().to_string();
    if !known(st, &id) {
        return;
    }
    let Ok(health) = serde_json::from_value(v["health"].clone()) else { return };
    with_seen(st, &id, |s| s.host_health = Some(health));
    st.direct.announce_health(st, &id);
}

fn stats(st: &Arc<Station>, v: &Value) {
    let Some(rows) = v["shows"].as_array() else { return };
    let ids = st.registry.lock().ids();
    let mut seen = st.direct.seen.lock();
    for row in rows {
        let Some(id) = row["id"].as_str().filter(|id| ids.iter().any(|k| k == id)) else { continue };
        seen.entry(id.to_string()).or_default().take_stats(row);
    }
}
