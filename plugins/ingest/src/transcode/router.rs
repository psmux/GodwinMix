//! Where a converted stream's tags go: one publication per distinct pair of
//! video and sound, on the renditions hub, which every destination that
//! wants that pair reads through a bounded queue of its own.
//!
//! A pair's video is an encoder's output or the stream's own video; its
//! sound is an audio encoder's or the stream's own. Nothing is sent for a
//! pair until its video has a keyframe, and no sound older than that
//! keyframe, so every destination starts on a picture it can decode.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};

use rml_rtmp::sessions::StreamMetadata;
use serde_json::Value;

use super::sink::Route;
use crate::flv;
use crate::hub::{Hub, Publication};
use crate::media_tag::{MediaTag, TagKind};

/// One pair a destination wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub key: String,
    pub video: Option<String>,
    pub audio: Option<String>,
}

struct Open {
    output: Output,
    publication: Publication,
    /// The time of the keyframe it started on.
    started: Option<u32>,
}

#[derive(Default)]
struct Inner {
    wanted: Vec<Output>,
    open: Vec<Open>,
    live: bool,
    /// The newest sequence header of each producer, for a pair opened late.
    headers: HashMap<String, MediaTag>,
    script: Option<MediaTag>,
    /// Each node's description, for the metadata of a pair it feeds.
    nodes: HashMap<String, Value>,
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
            for producer in [&output.video, &output.audio].into_iter().flatten() {
                if let Some(h) = inner.headers.get(producer) {
                    publication.push(h.clone());
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
            TagKind::Audio => self.tag(&format!("copy:{stream}:audio"), tag),
        }
    }
}

impl Route for Router {
    fn tag(&self, node: &str, tag: MediaTag) {
        let mut inner = self.lock();
        if tag.sequence_header {
            inner.headers.insert(node.to_string(), tag.clone());
        }
        for o in inner.open.iter_mut() {
            let video = o.output.video.as_deref() == Some(node);
            let audio = o.output.audio.as_deref() == Some(node);
            if tag.sequence_header && (video || audio) {
                o.publication.push(tag.clone());
            } else if video {
                if o.started.is_none() && !tag.keyframe {
                    continue;
                }
                o.started.get_or_insert(tag.timestamp_ms);
                o.publication.push(tag.clone());
            } else if audio {
                let late = o.output.video.is_some() && o.started.is_none_or(|at| tag.timestamp_ms < at);
                if !late {
                    o.publication.push(tag.clone());
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

/// `onMetaData` for a pair: the publisher's own, with the size, rate and
/// bit rates of whatever this pair encodes put in their place.
fn metadata(inner: &Inner, output: &Output, at: u32) -> MediaTag {
    let mut meta = inner
        .script
        .as_ref()
        .and_then(|s| crate::restream::meta::parse(&s.payload))
        .unwrap_or_else(StreamMetadata::new);
    if let Some(v) = output.video.as_ref().and_then(|id| inner.nodes.get(id)) {
        let n = |k: &str| v.get(k).and_then(Value::as_u64).map(|x| x as u32);
        meta.video_width = n("width");
        meta.video_height = n("height");
        meta.video_bitrate_kbps = n("bitrate_kbps");
        meta.video_codec_id = Some(7);
        let fps = v.get("fps").and_then(Value::as_array).and_then(|p| Some(p.first()?.as_f64()? / p.get(1)?.as_f64()?.max(1.0)));
        meta.video_frame_rate = fps.map(|f| f as f32);
    }
    if let Some(a) = output.audio.as_ref().and_then(|id| inner.nodes.get(id)) {
        let n = |k: &str| a.get(k).and_then(Value::as_u64).map(|x| x as u32);
        meta.audio_bitrate_kbps = n("bitrate_kbps");
        meta.audio_sample_rate = n("sample_rate");
        meta.audio_channels = n("channels");
        meta.audio_codec_id = Some(10);
    }
    if output.audio.is_none() {
        (meta.audio_codec_id, meta.audio_bitrate_kbps, meta.audio_channels, meta.audio_sample_rate) = (None, None, None, None);
    }
    let payload = flv::metadata_body(&meta);
    MediaTag { kind: TagKind::Script, timestamp_ms: at, keyframe: false, sequence_header: false, payload: payload.into() }
}
