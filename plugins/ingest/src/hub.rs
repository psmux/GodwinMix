//! The hub: every stream on the shared RTMP listener, by `(app, stream)`, and
//! everyone reading one.
//!
//! ```text
//!   publisher thread ──push──► Slot ──offer──► queue ──recv──► reader thread
//!                               │ headers        (bounded,       (a mixer source's
//!                               │ meter           drops GOPs)     relay, a restream)
//! ```
//!
//! A publisher pushes each tag once. The payload is an `Arc<[u8]>`, so a
//! tag reaches every reader as a pointer and never as a copy. Each reader has
//! a queue of its own with a ceiling; one that falls behind loses whole GOPs
//! from the front and picks up again at the next keyframe, and the loss is
//! counted. Nothing a reader does can make a publisher wait: the only lock a
//! publisher takes is the slot's, and no reader holds that while it does I/O.
//!
//! A reader may subscribe before anybody publishes. It waits, and gets the
//! next publisher from its first keyframe. When that publisher leaves the
//! reader is told `Ended` and is done; the next session needs a new
//! subscription, which is what keeps a reader from splicing two publishers'
//! timelines together.

mod meter;
mod queue;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde_json::{json, Value};

pub use queue::Recv;

use crate::media_tag::{MediaTag, TagKind};

type Key = (String, String);

/// The registry. Cloning it clones a handle to the same one.
#[derive(Clone, Default)]
pub struct Hub {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    slots: Mutex<HashMap<Key, Arc<Slot>>>,
    sessions: AtomicU64,
}

/// One stream name, live or waited for.
struct Slot {
    key: Key,
    state: Mutex<SlotState>,
}

#[derive(Default)]
struct SlotState {
    session: Option<Session>,
    readers: Vec<Arc<queue::Shared>>,
    headers: queue::Headers,
    /// GOPs lost by readers that have already gone, this session.
    dropped_gops: u64,
}

struct Session {
    id: u64,
    from: String,
    key: Option<String>,
    meter: meter::Meter,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Hub {
    pub fn new() -> Hub {
        Hub::default()
    }

    /// The slot for a name, made if need be, with the registry still locked
    /// so a reader leaving cannot take it out of the map in between.
    fn slot<'a>(
        slots: &mut MutexGuard<'a, HashMap<Key, Arc<Slot>>>,
        app: &str,
        stream: &str,
    ) -> Arc<Slot> {
        let key = (app.to_string(), stream.to_string());
        slots
            .entry(key.clone())
            .or_insert_with(|| Arc::new(Slot { key, state: Mutex::default() }))
            .clone()
    }

    /// Start a session. Refused while somebody else is publishing that name.
    pub fn publish(
        &self,
        app: &str,
        stream: &str,
        from: &str,
        key: Option<String>,
    ) -> Result<Publication, String> {
        let mut slots = lock(&self.inner.slots);
        let slot = Hub::slot(&mut slots, app, stream);
        let mut state = lock(&slot.state);
        drop(slots);
        if let Some(live) = &state.session {
            return Err(format!(
                "{app}/{stream} is already being published from {}. Give this encoder \
                 another stream name, or stop the other one first.",
                live.from
            ));
        }
        let id = self.inner.sessions.fetch_add(1, Ordering::Relaxed) + 1;
        let session = Session { id, from: from.to_string(), key, meter: meter::Meter::new() };
        state.session = Some(session);
        state.headers = queue::Headers::default();
        state.dropped_gops = 0;
        drop(state);
        Ok(Publication { inner: self.inner.clone(), slot, id })
    }

    /// Read a stream: now if it is live, from its next publisher if not.
    ///
    /// The restreamer's door in. It takes the reader as an iterator of
    /// `MediaTag`, which ends when the publisher leaves.
    pub fn subscribe(&self, app: &str, stream: &str) -> Reader {
        let mut slots = lock(&self.inner.slots);
        let slot = Hub::slot(&mut slots, app, stream);
        let shared = Arc::new(queue::Shared::new());
        lock(&slot.state).readers.push(shared.clone());
        drop(slots);
        Reader { inner: self.inner.clone(), slot, shared }
    }

    pub fn is_live(&self, app: &str, stream: &str) -> bool {
        let slot = lock(&self.inner.slots).get(&(app.to_string(), stream.to_string())).cloned();
        slot.is_some_and(|s| lock(&s.state).session.is_some())
    }

    /// Every live stream, in the shape `streams` answers with.
    pub fn streams(&self) -> Vec<Value> {
        let slots: Vec<Arc<Slot>> = lock(&self.inner.slots).values().cloned().collect();
        let mut out: Vec<Value> = slots.iter().filter_map(|s| describe(s)).collect();
        let name = |v: &Value| format!("{}/{}", v["app"].as_str().unwrap_or(""), v["stream"].as_str().unwrap_or(""));
        out.sort_by_key(name);
        out
    }

    /// One live stream, described.
    pub fn stream(&self, app: &str, stream: &str) -> Option<Value> {
        let slot = lock(&self.inner.slots).get(&(app.to_string(), stream.to_string())).cloned()?;
        describe(&slot)
    }
}

fn describe(slot: &Slot) -> Option<Value> {
    let state = lock(&slot.state);
    let session = state.session.as_ref()?;
    let (video, audio) = session.meter.describe();
    let dropped: u64 =
        state.dropped_gops + state.readers.iter().map(|r| r.dropped_gops()).sum::<u64>();
    Some(json!({
        "app": slot.key.0,
        "stream": slot.key.1,
        "state": "live",
        "since_ms": session.meter.since_ms,
        "from": session.from,
        "key": session.key,
        "video": video,
        "audio": audio,
        "readers": state.readers.len(),
        "dropped_gops": dropped,
        "bytes": session.meter.total_bytes,
    }))
}

/// A publisher's end. Dropping it ends the session and every reader of it.
pub struct Publication {
    inner: Arc<Inner>,
    slot: Arc<Slot>,
    id: u64,
}

impl Publication {
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
    inner: Arc<Inner>,
    slot: Arc<Slot>,
    shared: Arc<queue::Shared>,
}

impl Reader {
    /// The next tag, the end, or nothing yet.
    pub fn recv_timeout(&self, wait: Duration) -> Recv {
        self.shared.recv(wait)
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

#[cfg(test)]
#[path = "hub/tests.rs"]
mod tests;
