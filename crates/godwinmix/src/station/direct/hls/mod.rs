//! HLS from a show without compositing, served by the station from its own
//! port the way a show serves its `hls/output`.
//!
//! ```text
//!   direct host: input ──► hub "direct.<id>/main" ──► relay (loopback) ──┐
//!                (and a rendition's pair, when one is asked for)          │
//!                                                                         ▼
//!   station:  one packager thread per HLS output ──► appsrc ──► hls::attach (cmafmux, ring)
//!                                                                         │
//!   player ──► /hls/<output>/master.m3u8?show=<id>&key=... ◄──────────────┘
//! ```
//!
//! Why here and not in the direct host: the packager, the rings, the
//! playlists, LL-HLS, DASH and the viewer keys already exist once, in the
//! engine and the control port's `/hls` routes, and the station links both.
//! Doing it in the host would mean writing all of that again inside a
//! plugin, or files on disk the station then serves. Nothing is decoded
//! for it: the relay hands over the input's own frames, or the pair a
//! rendition made, and `cmafmux` repackages them.
//!
//! A packager runs only while its output is on and its show is in the
//! table, so nothing runs unless asked. Each one is a thread of its own and
//! a pipeline of its own; one that fails reports it on its own output.

mod board;
pub mod caps;
pub mod edit;
mod flv;
mod packager;
mod serve;
mod session;
pub mod spec;

pub use serve::router;

use super::outputs;
use crate::station::state::Station;
use godwinmix_core::hls::Stream;
use godwinmix_protocol::destination::{DestinationLive, Playback, StoredDestination};
use packager::{Packager, Source};
use parking_lot::Mutex;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use tracing::warn;

/// Every HLS output of every direct show, by show and output id.
#[derive(Default)]
pub struct Packagers {
    running: Mutex<BTreeMap<(String, String), Packager>>,
}

/// What one output should be doing now.
struct Want {
    spec: spec::HlsSpec,
    source: Option<Source>,
    why_not: Option<String>,
}

impl Packagers {
    /// Make the running packagers match the shows: start what is new, stop
    /// what went, and start again what now reads something else. Called on
    /// the table's thread after every table, never on a handler.
    pub fn apply(&self, st: &Station) {
        let wanted = wants(st);
        let mut running = self.running.lock();
        running.retain(|k, _| wanted.contains_key(k));
        for ((show, output), want) in wanted {
            let key = (show.clone(), output.clone());
            let same = |p: &Packager| p.source == want.source && p.stream.params == want.spec.params && keyed(p, &want);
            // One still waiting is started again: why it waits may have moved.
            if running.get(&key).is_some_and(|p| same(p) && p.source.is_some()) {
                continue;
            }
            // A stream already published keeps its rings, so a player sees
            // one discontinuity and not a new stream.
            let stream = match running.get(&key) {
                Some(p) if p.stream.params == want.spec.params && keyed(p, &want) => p.stream.clone(),
                _ => match new_stream(&show, &output, &want.spec) {
                    Ok(s) => s,
                    Err(e) => {
                        warn!(%show, %output, error = %e, "no viewer key for an HLS output");
                        continue;
                    }
                },
            };
            let sound = refusal(&show, &output);
            running.insert(key, Packager::start(stream, want.source, want.why_not, sound));
        }
    }

    pub fn stream(&self, show: &str, output: &str) -> Option<Arc<Stream>> {
        self.running.lock().get(&(show.to_string(), output.to_string())).map(|p| p.stream.clone())
    }

    /// The HLS outputs one show serves now.
    pub fn ids(&self, show: &str) -> Vec<String> {
        self.running.lock().keys().filter(|(s, _)| s == show).map(|(_, o)| o.clone()).collect()
    }

    /// What one output is doing, and where a player opens it.
    pub fn view(&self, show: &str, output: &str) -> Option<(DestinationLive, Playback)> {
        let running = self.running.lock();
        let p = running.get(&(show.to_string(), output.to_string()))?;
        let q = format!("show={show}&key={}", p.stream.viewer_key);
        let playback = Playback {
            master_url_path: format!("/hls/{output}/master.m3u8?{q}"),
            dash_url_path: format!("/hls/{output}/manifest.mpd?{q}"),
            viewers: p.stream.viewers.count() as u32,
        };
        Some((p.board.read(), playback))
    }
}

fn keyed(p: &Packager, want: &Want) -> bool {
    want.spec.viewer_key.as_ref().is_none_or(|k| *k == p.stream.viewer_key)
}

fn new_stream(show: &str, output: &str, spec: &spec::HlsSpec) -> anyhow::Result<Arc<Stream>> {
    let key = match &spec.viewer_key {
        Some(k) => k.clone(),
        None => godwinmix_core::hls::key::viewer_key(&format!("{show}/{output}"))?,
    };
    Ok(Arc::new(Stream::new(output, spec.params, &key)))
}

/// The sentence for sound fragmented MP4 does not carry, naming the call
/// that fixes it.
fn refusal(show: &str, output: &str) -> impl Fn(&str) -> String + Send + 'static {
    let (show, output) = (show.to_string(), output.to_string());
    move |codec| {
        format!(
            "the input's sound is {codec}, and HLS carries AAC: copying it would make segments no player can play. \
             Give the output a rendition with AAC sound, show.output.set {{id: \"{show}\", output: \"{output}\", \
             rendition: {{\"audio\": {{\"codec\": \"aac\"}}}}}}, and the picture is still copied."
        )
    }
}

/// Every HLS output that should run, with what it reads.
fn wants(st: &Station) -> BTreeMap<(String, String), Want> {
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
    if d.rendition.is_none() {
        return (Some(Source { relay, path: format!("{app}/main") }), None);
    }
    let Some(row) = st.direct.transcode.row(show, d, json!({})) else {
        let (_, refused) = st.direct.transcode.view(show, &d.id);
        return (None, Some(refused.map(|r| r.message).unwrap_or_else(|| "the rendition was refused".into())));
    };
    let (video, audio) = (row["video"].as_str(), row["audio"].as_str());
    if video.is_none() && audio.is_none() {
        return (None, Some("waiting for the rendition to be planned".into()));
    }
    let main = row["stream"].as_str().unwrap_or("main");
    let key = format!("{main}|{}|{}", video.unwrap_or("-"), audio.unwrap_or("-"));
    (Some(Source { relay, path: format!("{app}/{key}") }), None)
}
