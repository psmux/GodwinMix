//! A publisher let in: its tags on their way into the hub, and the core told
//! when its codecs are known and when it leaves.

use std::sync::Arc;

use godwinmix_sdk::plugin::Reporter;
use serde_json::json;

use super::OnAir;
use crate::hub::Publication;
use crate::media_tag::MediaTag;
use crate::rtmp::{self, Inlet};

/// What an inlet needs of the gate after it has been let in.
pub struct Parts {
    pub reporter: Option<Reporter>,
    pub relay: String,
}

impl Parts {
    pub fn live(&self, channel: &str, publication: &Publication) {
        let Some(r) = &self.reporter else { return };
        let mut described = publication.describe().unwrap_or_else(|| json!({}));
        described["channel"] = json!(channel);
        described["relay"] = json!(self.relay);
        r.event("channel.stream", described);
    }
}

/// One publisher's tags on their way into the hub.
pub struct Stream {
    pub publication: Option<Publication>,
    /// Set on the open door, which announces arrivals the old way.
    pub open: Option<rtmp::Publisher>,
    pub gate: Parts,
}

impl Stream {
    fn push(&mut self, tag: MediaTag) -> bool {
        self.publication.as_ref().is_some_and(|p| p.push(tag))
    }
}

impl Inlet for Stream {
    fn tag(&mut self, tag: MediaTag) {
        self.push(tag);
    }

    fn wants_keyframe(&self) -> bool {
        self.publication.as_ref().is_some_and(|p| p.wants_keyframe())
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        // The session ends before anyone is told it has, so nobody who asks
        // straight after the event still sees it live.
        self.publication = None;
        if let (Some(who), Some(r)) = (&self.open, &self.gate.reporter) {
            r.info(format!("{}/{} stopped publishing", who.app, who.key));
            r.event(
                "ingest.publisher",
                json!({"action": "left", "id": who.slug(), "name": format!("{}/{}", who.app, who.key)}),
            );
        }
    }
}

/// A stream on a channel: the same, and the core is told about its codecs
/// and its leaving.
pub struct Channelled {
    pub channel: String,
    pub app: String,
    pub name: String,
    pub stream: Stream,
    pub on_air: (Arc<OnAir>, u64),
}

impl Inlet for Channelled {
    fn tag(&mut self, tag: MediaTag) {
        if self.stream.push(tag) {
            if let Some(p) = &self.stream.publication {
                self.stream.gate.live(&self.channel, p);
            }
        }
    }

    fn wants_keyframe(&self) -> bool {
        self.stream.wants_keyframe()
    }
}

impl Drop for Channelled {
    fn drop(&mut self) {
        self.on_air.0.remove(self.on_air.1);
        // A session a new publisher took over is not the stream leaving: the
        // name is live again already, and saying idle now would tell the core
        // the opposite of the truth. See `hub::takeover`.
        let replaced = self.stream.publication.as_ref().is_some_and(|p| !p.current());
        self.stream.publication = None;
        let Some(r) = &self.stream.gate.reporter else { return };
        if replaced {
            r.info(format!("{}/{}: the publisher that went quiet was cut off", self.app, self.name));
            return;
        }
        r.info(format!("{}/{} stopped publishing", self.app, self.name));
        r.event(
            "channel.stream",
            json!({"channel": self.channel, "app": self.app, "stream": self.name, "state": "idle"}),
        );
    }
}
