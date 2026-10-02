//! The two ends of a stream: the publisher's, and each reader's.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use super::{describe, lock, queue, Inner, Recv, Slot};
use crate::media_tag::{MediaTag, TagKind};

/// A publisher's end. Dropping it ends the session and every reader of it.
pub struct Publication {
    pub(super) inner: Arc<Inner>,
    pub(super) slot: Arc<Slot>,
    pub(super) id: u64,
    /// Where the quiet publisher this one took the name from was, if it did.
    pub(super) took_over: Option<String>,
}

impl Publication {
    /// The publisher this session replaced because it had gone quiet, if any.
    pub fn took_over(&self) -> Option<&str> {
        self.took_over.as_deref()
    }

    /// Whether this is still the session on its name. False once a new
    /// publisher has taken a quiet one over.
    pub fn current(&self) -> bool {
        lock(&self.slot.state).session.as_ref().is_some_and(|s| s.id == self.id)
    }

    /// Hand one tag to every reader. Returns true when the tag told the meter
    /// something new about the codecs.
    pub fn push(&self, tag: MediaTag) -> bool {
        let mut state = lock(&self.slot.state);
        let state = &mut *state;
        let Some(session) = state.session.as_mut().filter(|s| s.id == self.id) else {
            return false;
        };
        let news = session.meter.record(&tag);
        let audio_only = !session.meter.has_video();
        match tag.kind {
            TagKind::Script => state.headers.script = Some(tag.clone()),
            TagKind::Video if tag.sequence_header => state.headers.video = Some(tag.clone()),
            TagKind::Audio if tag.sequence_header => state.headers.audio = Some(tag.clone()),
            _ => {}
        }
        for reader in &state.readers {
            reader.offer(&tag, &state.headers, audio_only);
        }
        news
    }

    /// Is any reader waiting for a keyframe? A WebRTC publisher sends one
    /// only when asked, so its session asks when this says so.
    pub fn wants_keyframe(&self) -> bool {
        lock(&self.slot.state).readers.iter().any(|r| r.wants_keyframe())
    }

    /// The stream as `streams` would describe it.
    pub fn describe(&self) -> Option<Value> {
        describe(&self.slot)
    }
}

impl Drop for Publication {
    fn drop(&mut self) {
        let mut slots = lock(&self.inner.slots);
        let mut state = lock(&self.slot.state);
        if state.session.as_ref().map(|s| s.id) != Some(self.id) {
            return;
        }
        state.session = None;
        state.headers = queue::Headers::default();
        for reader in state.readers.drain(..) {
            reader.end();
        }
        let idle = state.readers.is_empty();
        drop(state);
        if idle && slots.get(&self.slot.key).is_some_and(|s| Arc::ptr_eq(s, &self.slot)) {
            slots.remove(&self.slot.key);
        }
    }
}

/// A reader's end: an iterator of tags that ends with the session.
pub struct Reader {
    pub(super) inner: Arc<Inner>,
    pub(super) slot: Arc<Slot>,
    pub(super) shared: Arc<queue::Shared>,
}

impl Reader {
    /// The next tag, the end, or nothing yet.
    pub fn recv_timeout(&self, wait: Duration) -> Recv {
        self.shared.recv(wait)
    }

    /// Drop what waits, back to the newest keyframe.
    pub fn skip_to_latest_keyframe(&self) {
        self.shared.skip_to_latest_keyframe()
    }

    /// GOPs this reader lost by falling behind.
    pub fn dropped_gops(&self) -> u64 {
        self.shared.dropped_gops()
    }

    #[cfg(test)]
    pub fn waiting(&self) -> (usize, usize) {
        self.shared.waiting()
    }
}

impl Iterator for Reader {
    type Item = MediaTag;

    fn next(&mut self) -> Option<MediaTag> {
        loop {
            match self.shared.recv(Duration::from_secs(1)) {
                Recv::Tag(tag) => return Some(tag),
                Recv::Ended => return None,
                Recv::Timeout => continue,
            }
        }
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        let mut slots = lock(&self.inner.slots);
        let mut state = lock(&self.slot.state);
        let before = state.readers.len();
        state.readers.retain(|r| !Arc::ptr_eq(r, &self.shared));
        if state.readers.len() < before {
            state.dropped_gops += self.shared.dropped_gops();
        }
        let idle = state.session.is_none() && state.readers.is_empty();
        drop(state);
        let same = slots.get(&self.slot.key).is_some_and(|s| Arc::ptr_eq(s, &self.slot));
        if idle && same {
            slots.remove(&self.slot.key);
        }
    }
}
