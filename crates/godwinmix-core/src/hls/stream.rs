//! One HLS output as the server sees it: its rungs and its viewers.
//!
//! The control port finds one by id in the registry (`registry.rs`, re
//! exported here). It is process wide because the two halves never meet
//! otherwise: an output is built by the mixer thread from its config, and a
//! request arrives on the server's runtime with only an output id. The map is
//! read on every request and written when an output comes or goes, so it is
//! an `RwLock` held for a lookup and a clone of an `Arc`.

pub use super::registry::{egress_kbps, get, ids, publish, withdraw};

use super::playlist::{self, Report, Variant};
use super::track::{Track, TrackKind};
use super::viewers::Viewers;
use super::HlsParams;
use parking_lot::RwLock;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

pub struct Stream {
    pub id: String,
    pub params: HlsParams,
    /// The read only way in for viewers: `?key=` on a playlist opens this
    /// output's playlists and segments and nothing else, so a link can be
    /// shared without the control token that runs the mixer.
    pub viewer_key: String,
    tracks: RwLock<Vec<Arc<Track>>>,
    pub viewers: Viewers,
}

impl Stream {
    pub fn new(id: &str, params: HlsParams, viewer_key: &str) -> Stream {
        // A viewer counts for two windows after its last fetch.
        let horizon = Duration::from_secs(u64::from(params.window_s) * 2);
        Stream {
            id: id.to_string(),
            params,
            viewer_key: viewer_key.to_string(),
            tracks: RwLock::new(Vec::new()),
            viewers: Viewers::new(horizon),
        }
    }

    /// Whether `key` is this output's viewer key. Compared in constant time.
    pub fn admits(&self, key: &str) -> bool {
        let (a, b) = (self.viewer_key.as_bytes(), key.as_bytes());
        !a.is_empty() && a.len() == b.len() && a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
    }

    /// The multivariant playlist's path with the viewer key on it: what a
    /// page turns into a link on whatever host it was opened on.
    pub fn master_url_path(&self) -> String {
        format!("/hls/{}/master.m3u8?key={}", self.id, self.viewer_key)
    }

    /// A rung, or the audio. One already under this id and of this kind is
    /// kept, ring and all, so an output rebuilt after an error carries on
    /// numbering where it was and players see one discontinuity, not a new
    /// stream.
    pub fn add_track(&self, id: &str, kind: TrackKind, declared_kbps: u32) -> Arc<Track> {
        let mut tracks = self.tracks.write();
        if let Some(t) = tracks.iter().find(|t| t.id == id && t.kind == kind) {
            t.update_info(|i| i.declared_kbps = declared_kbps);
            return t.clone();
        }
        tracks.retain(|t| t.id != id);
        let track = Arc::new(Track::new(id, kind, self.params, declared_kbps));
        tracks.push(track.clone());
        track
    }

    pub fn remove_track(&self, id: &str) {
        self.tracks.write().retain(|t| t.id != id);
    }

    pub fn track(&self, id: &str) -> Option<Arc<Track>> {
        self.tracks.read().iter().find(|t| t.id == id).cloned()
    }

    pub fn tracks(&self) -> Vec<Arc<Track>> {
        self.tracks.read().clone()
    }

    /// Where every other video rung has got, for one rung's playlist.
    pub fn reports_for(&self, id: &str) -> Vec<Report> {
        self.tracks()
            .iter()
            .filter(|t| t.id != id && t.kind == TrackKind::Video)
            .filter_map(|t| {
                let pos = t.position();
                let (last_msn, last_part) = match pos.open {
                    Some((m, n)) if n > 0 => (m, Some(n - 1)),
                    _ => (pos.complete?, None),
                };
                Some(Report { id: t.id.clone(), last_msn, last_part })
            })
            .collect()
    }

    /// Every track has a whole segment and knows its codec, which is when a
    /// multivariant playlist can say anything true.
    pub fn ready(&self) -> bool {
        let tracks = self.tracks();
        !tracks.is_empty()
            && tracks.iter().all(|t| t.position().complete.is_some() && !t.info().codecs.is_empty())
    }

    /// What a playlist or an MPD says about one track. Peak bandwidth is what
    /// was measured over the window, or what the rendition asked for if that
    /// is higher.
    fn variant(t: &Arc<Track>) -> Variant {
        let info = t.info();
        let declared = u64::from(info.declared_kbps) * 1000;
        let (peak, average) = t.measured_bps().map(|(p, a)| (p, Some(a))).unwrap_or((0, None));
        Variant { id: t.id.clone(), bandwidth: peak.max(declared).max(1), average, info }
    }

    /// The multivariant playlist.
    pub fn master(&self) -> String {
        let tracks = self.tracks();
        let video: Vec<Variant> = tracks.iter().filter(|t| t.kind == TrackKind::Video).map(Self::variant).collect();
        let audio = tracks.iter().find(|t| t.kind == TrackKind::Audio).map(Self::variant);
        playlist::master(&video, audio.as_ref())
    }

    /// The same tracks as a DASH MPD, with `query` on every URL. None until
    /// there is a whole segment to list.
    pub fn mpd(&self, now_ms: i64, query: &str) -> Option<String> {
        let tracks = self.tracks();
        let parts: Vec<(Variant, TrackKind, super::ring::View)> =
            tracks.iter().map(|t| (Self::variant(t), t.kind, t.view())).collect();
        let reps: Vec<super::dash::Rep<'_>> = parts
            .iter()
            .map(|(v, kind, view)| super::dash::Rep { id: &v.id, kind: *kind, info: &v.info, view, bandwidth: v.bandwidth })
            .collect();
        super::dash::mpd(&reps, &self.params, now_ms, query)
    }

    pub fn memory(&self) -> usize {
        self.tracks().iter().map(|t| t.memory()).sum()
    }

    /// What `output.list` shows for this output.
    pub fn status(&self) -> Value {
        let rungs: Vec<Value> = self
            .tracks()
            .iter()
            .map(|t| {
                let info = t.info();
                json!({
                    "id": t.id,
                    "kind": if t.kind == TrackKind::Video { "video" } else { "audio" },
                    "codecs": info.codecs,
                    "width": info.width,
                    "height": info.height,
                    "segments": t.view().segments.iter().filter(|s| s.complete).count(),
                    "last_msn": t.position().newest(),
                    "memory_bytes": t.memory(),
                })
            })
            .collect();
        let viewers = self.viewers.count();
        json!({
            "playback": {
                "master_url_path": self.master_url_path(),
                "dash_url_path": format!("/hls/{}/manifest.mpd?key={}", self.id, self.viewer_key),
                "viewers": viewers,
            },
            "low_latency": self.params.low_latency(),
            "segment_ms": self.params.segment_ms,
            "part_ms": self.params.part_ms,
            "window": self.params.window_s,
            "viewers": viewers,
            "egress_kbps": self.viewers.egress_kbps(),
            "memory_bytes": self.memory(),
            "rungs": rungs,
        })
    }
}
