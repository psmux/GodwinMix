//! Which HLS outputs should run, and what each one reads.

use super::packager::Source;
use super::spec;
use crate::station::direct::outputs;
use crate::station::state::Station;
use godwinmix_protocol::destination::StoredDestination;
use serde_json::json;
use std::collections::BTreeMap;

/// What one output should be doing now.
pub struct Want {
    pub spec: spec::HlsSpec,
    pub source: Option<Source>,
    pub why_not: Option<String>,
}

/// Every HLS output that should run, with what it reads.
pub fn wants(st: &Station) -> BTreeMap<(String, String), Want> {
    let records: Vec<_> = st.registry.lock().records.iter().filter(|r| !r.stopped && !r.compositing && r.input.is_some()).cloned().collect();
    let mut out = BTreeMap::new();
    for r in records {
        if !r.outputs.iter().any(|o| o.enabled && o.platform == spec::SCHEME) {
            continue;
        }
        let relay = st.direct.seen.lock().get(&r.id).and_then(|s| s.relay());
        for d in outputs::stored(&r.id, &r.outputs).into_iter().filter(|d| d.enabled && d.platform == spec::SCHEME) {
            let (source, why_not) = source_of(st, &r.id, &d, relay.as_ref());
            out.insert((r.id.clone(), d.id.clone()), Want { spec: spec::read(&d.server), source, why_not });
        }
    }
    out
}

/// Where one output reads from: the show's stream, or its rendition's
/// pair once the plan has made one. None, and why, until both are known.
fn source_of(st: &Station, show: &str, d: &StoredDestination, relay: Option<&(String, String)>) -> (Option<Source>, Option<String>) {
    let Some((addr, stream)) = relay else { return (None, Some("waiting for the input to go live".into())) };
    let Ok(relay) = addr.parse() else { return (None, Some(format!("the direct host gave {addr} as its relay, which is not an address"))) };
    let app = stream.split_once('/').map(|(a, _)| a).unwrap_or(stream);
    let copy = Source { relay, path: format!("{app}/main") };
    if d.rendition.is_none() {
        return (Some(copy), None);
    }
    let (plan, refused) = st.direct.transcode.view(show, &d.id);
    let Some(row) = st.direct.transcode.row(show, d, json!({})) else {
        return (None, Some(refused.map(|r| r.message).unwrap_or_else(|| "the rendition was refused".into())));
    };
    let (video, audio) = (row["video"].as_str(), row["audio"].as_str());
    match (video.is_none() && audio.is_none(), plan.is_some()) {
        // The input already is what the rendition asks for: a copy.
        (true, true) => return (Some(copy), None),
        (true, false) => return (None, Some("waiting for the input's shape, to plan the rendition against".into())),
        _ => {}
    }
    let main = row["stream"].as_str().unwrap_or("main");
    let key = format!("{main}|{}|{}", video.unwrap_or("-"), audio.unwrap_or("-"));
    (Some(Source { relay, path: format!("{app}/{key}") }), None)
}
