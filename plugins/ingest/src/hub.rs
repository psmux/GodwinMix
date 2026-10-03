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

mod ends;
mod meter;
mod queue;
mod takeover;

use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::{json, Value};

pub use ends::{Publication, Reader};
pub use queue::Recv;
pub use takeover::STALE;
#[cfg(all(test, unix))]
pub use takeover::HESITATE;


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
    /// `rtmp`, `rtmps`, `srt` or `whip`: how it arrived.
    via: &'static str,
    meter: meter::Meter,
    /// Cuts the publisher off. Set for a publisher the gate let in, and what
    /// lets a quiet session be taken over; see `takeover`.
    kick: Option<crate::rtmp::Kick>,
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

    /// Start an RTMP session. Refused while somebody else is publishing that
    /// name.
    pub fn publish(&self, app: &str, stream: &str, from: &str, key: Option<String>) -> Result<Publication, String> {
        self.publish_via(app, stream, from, key, "rtmp")
    }

    /// Start a session that arrived over `via`, with no means to cut it off,
    /// so it is never taken over. Every protocol ends up in `publish_with`, so
    /// a stream is a stream to every reader whatever carried it.
    pub fn publish_via(
        &self,
        app: &str,
        stream: &str,
        from: &str,
        key: Option<String>,
        via: &'static str,
    ) -> Result<Publication, String> {
        self.publish_with(app, stream, from, key, via, None)
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
        "protocol": session.via,
        "video": video,
        "audio": audio,
        "readers": state.readers.len(),
        "dropped_gops": dropped,
        "bytes": session.meter.total_bytes,
    }))
}

#[cfg(test)]
#[path = "hub/tests.rs"]
mod tests;
