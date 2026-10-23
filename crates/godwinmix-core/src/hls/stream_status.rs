//! What `output.list` and `show.list` say of one HLS output.

use super::super::access::Access;
use super::super::track::TrackKind;
use super::Stream;
use serde_json::{json, Value};

impl Stream {
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
            // Who may play it, as `params` takes it, so an edit form opens on
            // what is in force rather than on the default.
            "access": if self.params.access == Access::Keyed { "keyed" } else { "open" },
            "open_public": self.params.access == Access::OpenPublic,
            "viewers": viewers,
            "egress_kbps": self.viewers.egress_kbps(),
            "memory_bytes": self.memory(),
            "rungs": rungs,
        })
    }
}
