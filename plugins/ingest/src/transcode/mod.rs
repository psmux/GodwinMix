//! Converting a channel's stream for the destinations that asked for a
//! rendition. The one place this plugin decodes anything.
//!
//! The core plans (`godwinmix-render`) and admits (`godwinmix-govern`) and
//! hands the result over in the channel table: for each stream, the nodes to
//! build with their elements and properties, and for each converting
//! destination, the pair of producers it reads. This builds them.
//!
//! ```text
//!   hub ──► one reader per converted stream ──► decode once ──► scale per size ──► encode per rendition
//!                                                                                        │
//!   renditions hub (one publication per pair of video and sound) ◄─────────────────────┘
//!        │
//!        └──► the ordinary restream senders, one per destination, as for a copy
//! ```
//!
//! A destination that asked for nothing, or that the plan copies, never
//! comes near any of this: it reads the hub directly, as it always has.
//! `docs/explanation/channel-transcoding.md` says why the work is split this
//! way between the core and this process.

mod build;
mod graph;
pub(crate) mod input;
mod router;
mod session;
mod sink;
mod spec;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use crate::hub::Hub;
use crate::sends::{Feed, Wanted};
pub use router::{Output, Tap};
use session::Session;
pub use spec::{specs, stream_specs, StreamSpec};

/// The name a pair is published under on the renditions hub.
pub fn output_key(stream: &str, video: Option<&str>, audio: Option<&str>) -> String {
    format!("{stream}|{}|{}", video.unwrap_or("-"), audio.unwrap_or("-"))
}

/// Every stream being converted.
pub struct Transcoders {
    hub: Hub,
    renditions: Hub,
    running: Mutex<HashMap<(String, String), Session>>,
}

impl Transcoders {
    pub fn new(hub: Hub) -> Transcoders {
        Transcoders { hub, renditions: Hub::new(), running: Mutex::default() }
    }

    /// Where converted pairs are published, for the senders to read.
    pub fn renditions(&self) -> Hub {
        self.renditions.clone()
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<(String, String), Session>> {
        self.running.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Convert what `specs` asks for, for the destinations in `wanted`. A
    /// stream no longer asked for stops; one still asked for is changed in
    /// place, node by node.
    pub fn apply(&self, specs: Vec<StreamSpec>, wanted: &[Wanted]) {
        let mut running = self.lock();
        running.retain(|(app, stream), _| specs.iter().any(|s| &s.app == app && &s.stream == stream));
        for s in specs {
            let outputs = outputs(&s, wanted);
            let key = (s.app.clone(), s.stream.clone());
            match running.get(&key) {
                Some(session) => session.update(s.nodes, outputs),
                None => {
                    let session = Session::start(self.hub.clone(), self.renditions.clone(), &s.app, &s.stream, s.nodes, outputs);
                    running.insert(key, session);
                }
            }
        }
    }

    /// Hand one converted stream's decoded pictures, about one a second, to
    /// `tap`, or to nobody. A stream not being converted has no pictures.
    pub fn set_tap(&self, app: &str, stream: &str, tap: Option<router::Tap>) {
        if let Some(session) = self.lock().get(&(app.to_string(), stream.to_string())) {
            session.set_tap(tap);
        }
    }

    /// Why a converting destination has nothing to send, when a node it
    /// reads would not run.
    pub fn error(&self, w: &Wanted) -> Option<String> {
        let Feed::Rendition { video, audio } = &w.feed else { return None };
        let running = self.lock();
        let session = running.get(&(w.app.clone(), w.stream.clone()))?;
        [video, audio].into_iter().flatten().find_map(|node| session.failed(node).or_else(|| session.failed("*")))
    }
}

/// The pairs the destinations of one stream read.
fn outputs(s: &StreamSpec, wanted: &[Wanted]) -> Vec<Output> {
    let mut out: Vec<Output> = Vec::new();
    for w in wanted.iter().filter(|w| w.app == s.app && w.stream == s.stream) {
        let Feed::Rendition { video, audio } = &w.feed else { continue };
        if video.is_none() && audio.is_none() {
            continue;
        }
        let o = Output { key: output_key(&w.stream, video.as_deref(), audio.as_deref()), video: video.clone(), audio: audio.clone() };
        if !out.contains(&o) {
            out.push(o);
        }
    }
    out
}
