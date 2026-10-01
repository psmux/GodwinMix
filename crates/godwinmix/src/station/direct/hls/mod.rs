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
mod feed;
mod flv;
mod packager;
mod serve;
mod session;
pub mod spec;
mod wants;

pub use serve::router;

use crate::station::state::Station;
use godwinmix_core::hls::Stream;
use godwinmix_protocol::destination::{DestinationLive, Playback, StoredDestination};
use godwinmix_protocol::error::RpcError;
use packager::Packager;
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

impl Packagers {
    /// Make the running packagers match the shows: start what is new, stop
    /// what went, and start again what now reads something else. Called on
    /// the table's thread after every table, never on a handler.
    pub fn apply(&self, st: &Station) {
        let wanted = wants::wants(st);
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

fn keyed(p: &Packager, want: &wants::Want) -> bool {
    want.spec.viewer_key.as_ref().is_none_or(|k| *k == p.stream.viewer_key)
}

fn new_stream(show: &str, output: &str, spec: &spec::HlsSpec) -> anyhow::Result<Arc<Stream>> {
    let key = match &spec.viewer_key {
        Some(k) => k.clone(),
        None => godwinmix_core::hls::key::viewer_key(&format!("{show}/{output}"))?,
    };
    Ok(Arc::new(Stream::new(output, spec.params, &key)))
}

/// Refuse a new HLS output that would copy sound already known not to be
/// AAC, and say what to add instead.
pub fn check_sound(st: &Station, show: &str, added: Option<&StoredDestination>) -> Result<(), RpcError> {
    let Some(d) = added.filter(|d| d.platform == spec::SCHEME && d.enabled && d.rendition.is_none()) else { return Ok(()) };
    let codec = st.direct.seen.lock().get(show).and_then(|s| s.input.as_ref()?["audio"]["codec"].as_str().map(str::to_string));
    let Some(codec) = codec.filter(|c| c != "aac") else { return Ok(()) };
    let msg = format!(
        "show {show}'s input sound is {codec}, and HLS carries AAC: a copy would make segments no player can play. \
         Add the output with rendition: {{\"audio\": {{\"codec\": \"aac\"}}}}, which converts the sound and still copies the picture."
    );
    let fix = json!({"audio": {"codec": "aac"}});
    Err(RpcError::invalid_params(msg).with("field", "rendition").with("output", d.id.as_str()).with("audio_codec", codec).with("rendition", fix))
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
