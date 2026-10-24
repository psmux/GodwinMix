//! Where a converted stream's tags go: one publication per distinct pair of
//! video and sound, on the renditions hub, which every destination that
//! wants that pair reads through a bounded queue of its own.
//!
//! A pair's video is an encoder's output or the stream's own video; its
//! sound is an audio encoder's or the stream's own. Nothing is sent for a
//! pair until its video has a keyframe, and no sound older than that
//! keyframe, so every destination starts on a picture it can decode.
//!
//! A pair may carry further sound tracks, each from a node of its own: they
//! go out as tracks 1, 2, 3 of the same publication, which only a reader of
//! every track is given (`Hub::subscribe_all`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};

use serde_json::Value;

use super::meta::metadata;
use super::sink::Route;
use crate::hub::{Hub, Publication};
use crate::media_tag::{MediaTag, TagKind};

/// One pair a destination wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub key: String,
    pub video: Option<String>,
    pub audio: Option<String>,
    /// The node of each further sound track, track 1 first.
    pub tracks: Vec<String>,
}

impl Output {
    /// The track a producer's tags go out as in this pair, if it feeds it:
    /// 0 for its video and first sound, 1, 2, 3 for the others.
    fn track_of(&self, node: &str) -> Option<u8> {
        if self.video.as_deref() == Some(node) || self.audio.as_deref() == Some(node) {
            return Some(0);
        }
        self.tracks.iter().position(|t| t == node).map(|i| u8::try_from(i + 1).unwrap_or(u8::MAX))
    }
}

/// `tag` as track `n`; the payload is shared, never copied.
fn numbered(tag: &MediaTag, n: u8) -> MediaTag {
    MediaTag { track: n, ..tag.clone() }
}

struct Open {
    output: Output,
    publication: Publication,
    /// The time of the keyframe it started on.
    started: Option<u32>,
}

#[derive(Default)]
pub(super) struct Inner {
    wanted: Vec<Output>,
    open: Vec<Open>,
    live: bool,
    /// The newest sequence header of each producer, for a pair opened late.
    pub(super) headers: HashMap<String, MediaTag>,
    pub(super) script: Option<MediaTag>,
    /// Each node's description, for the metadata of a pair it feeds.
    pub(super) nodes: HashMap<String, Value>,
}

/// Whether every AAC encoder a pair's sound comes from has given its header.
/// A pair starts on a keyframe only once they have: started on NVENC's
/// first keyframe, which comes before the first AAC frame, a reader (the
/// HLS packager) began without the sound, built its pipeline again when the
/// header came, and its MPEG-TS cutter never lined up with the CMAF after.
fn sound_ready(o: &Output, encodes: &[String], headers: &[String]) -> bool {
    o.audio.iter().chain(&o.tracks).filter(|n| encodes.contains(n)).all(|n| headers.contains(n))
}

/// Where a converted stream's decoded pictures go, when someone asked.
pub type Tap = std::sync::Arc<dyn Fn(&gstreamer::Sample) + Send + Sync>;

pub struct Router {
    hub: Hub,
    tap: Mutex<Option<Tap>>,
    app: String,
    base: AtomicU32,
    inner: Mutex<Inner>,
}

impl Router {
    pub fn new(hub: Hub, app: &str) -> Router {
        Router { hub, tap: Mutex::default(), app: app.to_string(), base: AtomicU32::new(0), inner: Mutex::default() }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Send the decoded pictures to `tap`, or to nobody.
    pub fn set_tap(&self, tap: Option<Tap>) {
        *self.tap.lock().unwrap_or_else(|e| e.into_inner()) = tap;
    }

    pub fn set_base(&self, ms: u32) {
        self.base.store(ms, Ordering::Relaxed);
    }

    pub fn set_nodes(&self, nodes: &[(String, Value)]) {
        self.lock().nodes = nodes.iter().cloned().collect();
    }

    /// The pairs destinations want now. While the stream is live, a new pair
    /// opens at once and a pair nobody wants any more ends.
    pub fn set_outputs(&self, wanted: Vec<Output>) {
        let mut inner = self.lock();
        inner.open.retain(|o| wanted.contains(&o.output));
        inner.wanted = wanted;
        if inner.live {
            self.open_missing(&mut inner);
        }
    }

    /// A session began: open every pair.
    pub fn start(&self) {
        let mut inner = self.lock();
        inner.live = true;
        self.open_missing(&mut inner);
    }

    /// The session ended: every pair ends, and its readers are told.
    pub fn stop(&self) {
        let mut inner = self.lock();
        inner.live = false;
        inner.open.clear();
        inner.headers.clear();
        inner.script = None;
    }

    fn open_missing(&self, inner: &mut Inner) {
        let missing: Vec<Output> = inner.wanted.iter().filter(|w| !inner.open.iter().any(|o| &o.output == *w)).cloned().collect();
        for output in missing {
            let Ok(publication) = self.hub.publish_via(&self.app, &output.key, "transcode", None, "transcode") else { continue };
            let script = metadata(inner, &output, self.base.load(Ordering::Relaxed));
            publication.push(script);
            for producer in [&output.video, &output.audio].into_iter().flatten().chain(&output.tracks) {
                if let (Some(h), Some(n)) = (inner.headers.get(producer), output.track_of(producer)) {
                    publication.push(numbered(h, n));
                }
            }
            inner.open.push(Open { output, publication, started: None });
        }
    }

    /// A tag of the stream itself: its own video and sound are the
    /// `copy:<stream>:<track>` producers.
    pub fn source(&self, stream: &str, tag: MediaTag) {
        match tag.kind {
            TagKind::Script => self.lock().script = Some(tag),
            TagKind::Video => self.tag(&format!("copy:{stream}:video"), tag),
            TagKind::Audio if tag.track == 0 => self.tag(&format!("copy:{stream}:audio"), tag),
            TagKind::Audio => {
                let node = format!("copy:{stream}:audio{}", tag.track);
                self.tag(&node, tag)
            }
        }
    }
}

impl Route for Router {
    fn tag(&self, node: &str, tag: MediaTag) {
        let mut inner = self.lock();
        if tag.sequence_header {
            inner.headers.insert(node.to_string(), tag.clone());
        }
        let headers: Vec<String> = inner.headers.keys().cloned().collect();
        let encodes: Vec<String> = inner.nodes.iter().filter(|(_, v)| v["kind"] == "aencode").map(|(k, _)| k.clone()).collect();
        for o in inner.open.iter_mut() {
            let video = o.output.video.as_deref() == Some(node);
            let Some(n) = o.output.track_of(node) else { continue };
            let audio = !video;
            if tag.sequence_header {
                o.publication.push(numbered(&tag, n));
            } else if video {
                if o.started.is_none() && (!tag.keyframe || !sound_ready(&o.output, &encodes, &headers)) {
                    continue;
                }
                o.started.get_or_insert(tag.timestamp_ms);
                o.publication.push(tag.clone());
            } else if audio {
                let late = o.output.video.is_some() && o.started.is_none_or(|at| tag.timestamp_ms < at);
                if !late {
                    o.publication.push(numbered(&tag, n));
                }
            }
        }
    }

    fn base_ms(&self) -> u32 {
        self.base.load(Ordering::Relaxed)
    }

    fn frame(&self, _node: &str, sample: &gstreamer::Sample) {
        let tap = self.tap.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(tap) = tap {
            tap(sample);
        }
    }
}
