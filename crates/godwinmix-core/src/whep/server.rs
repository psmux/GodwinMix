//! One `whep/output`'s viewers: the tees they hang off, the sessions, and a
//! watcher that ends the ones whose viewer has gone.
//!
//! The lock is never held while a viewer is negotiated (that waits up to
//! three seconds for ICE), and never while a session is taken down, so an
//! output rebuilding on the mixer thread never waits on a viewer.

use super::params::{VideoSend, WhepParams};
use super::session::{Session, Tees};
use super::Refusal;
use gstreamer as gst;
use gstreamer_webrtc::WebRTCPeerConnectionState as Peer;
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

/// How long a viewer may take to connect before it is let go.
const CONNECT_WITHIN: Duration = Duration::from_secs(20);

#[derive(Clone)]
struct Attached {
    pipeline: gst::Pipeline,
    video: gst::Element,
    audio: Option<gst::Element>,
    send: VideoSend,
    generation: u64,
}

struct Viewer {
    session: Session,
    since: Instant,
    generation: u64,
}

pub struct Server {
    pub id: String,
    pub params: WhepParams,
    /// What a viewer who has no control token presents instead.
    key: String,
    attached: Mutex<Option<Attached>>,
    viewers: Mutex<BTreeMap<String, Viewer>>,
    generation: AtomicU64,
    next: AtomicU64,
}

impl Server {
    pub fn new(id: &str, params: WhepParams, key: String) -> Arc<Server> {
        let server = Arc::new(Server {
            id: id.to_string(),
            params,
            key,
            attached: Mutex::new(None),
            viewers: Mutex::new(BTreeMap::new()),
            generation: AtomicU64::new(0),
            next: AtomicU64::new(1),
        });
        watch(Arc::downgrade(&server));
        server
    }

    /// Point new viewers at this pipeline's tees. Viewers of the pipeline
    /// before go with it; their players reconnect.
    pub fn attach(&self, pipeline: &gst::Pipeline, video: &gst::Element, audio: Option<&gst::Element>, send: VideoSend) {
        let generation = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        let attached = Attached { pipeline: pipeline.clone(), video: video.clone(), audio: audio.cloned(), send, generation };
        *self.attached.lock() = Some(attached);
    }

    /// No pipeline to hang viewers off: every session ends.
    pub fn detach(&self) {
        *self.attached.lock() = None;
        let gone = std::mem::take(&mut *self.viewers.lock());
        drop(gone);
    }

    /// Whether `key` is this output's viewer key, compared in constant time.
    pub fn admits(&self, key: &str) -> bool {
        let (a, b) = (self.key.as_bytes(), key.as_bytes());
        a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
    }

    pub fn ready(&self) -> bool {
        self.attached.lock().is_some()
    }

    pub fn viewers(&self) -> usize {
        self.viewers.lock().len()
    }

    /// A new viewer: its session id and the SDP answer.
    pub fn offer(&self, sdp: &str) -> Result<(String, String), Refusal> {
        let Some(at) = self.attached.lock().clone() else {
            return Err(Refusal::new(503, format!("whep/output '{}' is not running yet. It starts with the programme encoder; try again in a second.", self.id)));
        };
        let max = self.params.max_viewers as usize;
        if self.viewers() >= max {
            let why = "Raise max_viewers on the output, or serve a wider audience with an hls/output.";
            return Err(Refusal::new(503, format!("whep/output '{}' already has its {max} viewers. {why}", self.id)));
        }
        let n = self.next.fetch_add(1, Ordering::Relaxed);
        let session_id = format!("v{n}");
        let name = format!("whep-{}-{session_id}-g{}", self.id, at.generation);
        let tees = Tees { pipeline: &at.pipeline, video: &at.video, audio: at.audio.as_ref(), send: at.send };
        let (session, answer) = Session::start(&tees, sdp, &self.params, &name)?;
        if self.generation.load(Ordering::Relaxed) != at.generation {
            drop(session);
            return Err(Refusal::new(503, "the output was rebuilt while this viewer was being answered. POST the offer again."));
        }
        let viewer = Viewer { session, since: Instant::now(), generation: at.generation };
        self.viewers.lock().insert(session_id.clone(), viewer);
        Ok((session_id, answer))
    }

    /// The viewer said it is done (`DELETE`). Taken down outside the lock.
    pub fn end(&self, session: &str) -> bool {
        let gone = self.viewers.lock().remove(session);
        gone.is_some()
    }

    /// Take out every viewer that has gone, failed, never connected, or
    /// belongs to a pipeline that is no longer this output's.
    fn sweep(&self) {
        let current = self.generation.load(Ordering::Relaxed);
        let gone: Vec<Viewer> = {
            let mut map = self.viewers.lock();
            let dead: Vec<String> = map
                .iter()
                .filter(|(_, v)| {
                    let state = v.session.state();
                    let never = state != Peer::Connected && v.since.elapsed() > CONNECT_WITHIN;
                    v.generation != current || never || matches!(state, Peer::Failed | Peer::Closed | Peer::Disconnected)
                })
                .map(|(k, _)| k.clone())
                .collect();
            dead.iter().filter_map(|k| map.remove(k)).collect()
        };
        drop(gone);
    }
}

/// Once a second, for as long as the server exists.
fn watch(server: Weak<Server>) {
    let _ = std::thread::Builder::new().name("gmx-whep-watch".into()).spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        match server.upgrade() {
            Some(s) => s.sweep(),
            None => break,
        }
    });
}
