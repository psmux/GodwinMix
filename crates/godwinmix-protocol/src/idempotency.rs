//! Retries that cost nothing.
//!
//! Stripe's model, which 09 section 5 item 4 asks for: a mutating call carries
//! an `idempotency_key`, the core remembers the answer for 24 hours, and a
//! repeat gets the first answer back with `replayed: true`. A repeat under the
//! same key with different params is `-32602` with `data.idempotency:
//! "mismatch"`, because the alternative is silently answering a question
//! nobody asked.
//!
//! The key is reserved before the work starts, not after it finishes. A
//! lookup that found nothing and a store that happened afterwards left a
//! window in which two requests arriving together both ran: two sources
//! added, two outputs connected, one key. A caller that arrives while the
//! first is still running now waits on the same slot and gets the same
//! answer, which is the whole promise of the key.
//!
//! In memory, per process. A key that survived a restart would be answering
//! for a mixer that no longer holds the state the answer describes.

use crate::error::{ErrorCode, RpcError};
use serde_json::Value;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Notify;

/// How long an answer is worth replaying. The number in 03 section 6.
pub const TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Keys held at once before the oldest are dropped early. A key is small; a
/// client looping on failure is not, and an unbounded map in a process that
/// holds a live programme is a leak with a deadline.
const MAX_KEYS: usize = 4096;

/// How long a reservation may be held before another caller stops waiting on
/// it. Longer than any call is allowed to take, so this only fires when
/// something went wrong rather than when something was slow.
const RESERVATION_TTL: Duration = Duration::from_secs(crate::MAX_CALL_SECS * 3);

#[derive(Debug, Clone)]
struct Entry {
    method: String,
    params_hash: u64,
    body: Value,
    at: Instant,
}

/// A key is either being worked on or answered.
enum Slot {
    /// Somebody is running this call now. Waiters hold the same `Notify` and
    /// are woken when the slot becomes an answer or is released.
    InFlight { method: String, params_hash: u64, at: Instant, wake: Arc<Notify> },
    Done(Entry),
}

impl Slot {
    fn method(&self) -> &str {
        match self {
            Self::InFlight { method, .. } => method,
            Self::Done(e) => &e.method,
        }
    }

    fn params_hash(&self) -> u64 {
        match self {
            Self::InFlight { params_hash, .. } => *params_hash,
            Self::Done(e) => e.params_hash,
        }
    }

    fn stale(&self) -> bool {
        match self {
            Self::InFlight { at, .. } => at.elapsed() >= RESERVATION_TTL,
            Self::Done(e) => e.at.elapsed() >= TTL,
        }
    }

    fn age(&self) -> Duration {
        match self {
            Self::InFlight { at, .. } => at.elapsed(),
            Self::Done(e) => e.at.elapsed(),
        }
    }
}

#[derive(Default)]
pub struct Cache {
    entries: parking_lot::Mutex<HashMap<String, Slot>>,
}

/// What a reservation found.
pub enum Lookup {
    /// This caller owns the key. Do the work, then `commit` the answer, or
    /// drop the reservation to let the next caller try.
    Fresh(Reservation),
    /// The same call, already answered. Hand this body back.
    Replay(Value),
    /// Another caller is running this same call right now. Wait on this and
    /// reserve again; the second look finds the answer.
    InFlight(Arc<Notify>),
}

/// One caller's claim on a key.
///
/// Dropping it without committing releases the key and wakes whoever is
/// waiting, so a call that failed does not leave its key blocked for a day.
/// Only successful answers are stored: a failure the caller could fix and
/// retry must not be frozen.
pub struct Reservation {
    cache: Arc<Cache>,
    key: String,
    committed: bool,
}

impl Reservation {
    /// Remember this answer under the key and wake every waiter.
    pub fn commit(mut self, body: &Value) {
        self.committed = true;
        let mut entries = self.cache.entries.lock();
        let wake = match entries.get(&self.key) {
            Some(Slot::InFlight { wake, .. }) => Some(wake.clone()),
            _ => None,
        };
        let (method, params_hash) = match entries.get(&self.key) {
            Some(slot) => (slot.method().to_string(), slot.params_hash()),
            None => (String::new(), 0),
        };
        entries.insert(
            self.key.clone(),
            Slot::Done(Entry { method, params_hash, body: body.clone(), at: Instant::now() }),
        );
        prune(&mut entries);
        drop(entries);
        if let Some(wake) = wake {
            wake.notify_waiters();
        }
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let mut entries = self.cache.entries.lock();
        let wake = match entries.get(&self.key) {
            Some(Slot::InFlight { wake, .. }) => Some(wake.clone()),
            _ => None,
        };
        if wake.is_some() {
            entries.remove(&self.key);
        }
        drop(entries);
        if let Some(wake) = wake {
            wake.notify_waiters();
        }
    }
}

impl std::fmt::Debug for Lookup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fresh(_) => f.write_str("Fresh"),
            Self::Replay(body) => write!(f, "Replay({body})"),
            Self::InFlight(_) => f.write_str("InFlight"),
        }
    }
}

impl Cache {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Claim a key, or find out that somebody else has.
    ///
    /// The params are hashed rather than kept, because a body carrying a
    /// stream key should not sit in memory for a day.
    pub fn reserve(
        self: &Arc<Self>,
        key: &str,
        method: &str,
        params: &Value,
    ) -> Result<Lookup, RpcError> {
        let mut entries = self.entries.lock();
        prune(&mut entries);
        let hash = hash_params(params);
        if let Some(slot) = entries.get(key) {
            if slot.method() != method {
                return Err(mismatch(key, slot.method(), method));
            }
            if slot.params_hash() != hash {
                return Err(mismatch(key, method, method).with("params", "differ"));
            }
            match slot {
                Slot::InFlight { wake, .. } => return Ok(Lookup::InFlight(wake.clone())),
                Slot::Done(entry) => {
                    let mut body = entry.body.clone();
                    if let Some(map) = body.as_object_mut() {
                        map.insert("replayed".into(), Value::Bool(true));
                    }
                    return Ok(Lookup::Replay(body));
                }
            }
        }
        if entries.len() >= MAX_KEYS {
            drop_oldest(&mut entries);
        }
        entries.insert(
            key.to_string(),
            Slot::InFlight {
                method: method.to_string(),
                params_hash: hash,
                at: Instant::now(),
                wake: Arc::new(Notify::new()),
            },
        );
        Ok(Lookup::Fresh(Reservation {
            cache: self.clone(),
            key: key.to_string(),
            committed: false,
        }))
    }

    pub fn len(&self) -> usize {
        self.entries.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Answered keys only, which is what a test asserts on and what a metric
    /// would report.
    pub fn answered(&self) -> usize {
        self.entries.lock().values().filter(|s| matches!(s, Slot::Done(_))).count()
    }
}

fn prune(entries: &mut HashMap<String, Slot>) {
    entries.retain(|_, slot| !slot.stale());
}

fn drop_oldest(entries: &mut HashMap<String, Slot>) {
    // An answer goes before a reservation: somebody is still waiting on the
    // reservation and nobody is waiting on the answer.
    let oldest = entries
        .iter()
        .filter(|(_, s)| matches!(s, Slot::Done(_)))
        .max_by_key(|(_, s)| s.age())
        .map(|(k, _)| k.clone())
        .or_else(|| entries.iter().max_by_key(|(_, s)| s.age()).map(|(k, _)| k.clone()));
    if let Some(key) = oldest {
        entries.remove(&key);
    }
}

fn mismatch(key: &str, first: &str, now: &str) -> RpcError {
    RpcError::new(
        ErrorCode::InvalidParams,
        format!(
            "idempotency_key '{key}' was already used for {first} with different arguments, \
             and this is {now}. Use a new key, or send the identical call to replay the answer."
        ),
    )
    .with("idempotency", "mismatch")
    .with("key", key)
    .with("first_method", first)
}

/// Hash the params so the cache can tell "the same call again" from "a
/// different call under a reused key" without keeping the body.
///
/// Serialised through `serde_json::to_string` on a `Value`, whose object keys
/// are sorted, so the same call written with its keys in another order hashes
/// the same. The envelope keys the dispatcher handles are dropped first: a
/// retry that carries a fresh `trace_id` is still the same call.
fn hash_params(params: &Value) -> u64 {
    let mut cleaned = params.clone();
    if let Some(map) = cleaned.as_object_mut() {
        for key in ["trace_id", "idempotency_key", "confirm"] {
            map.remove(key);
        }
    }
    let text = serde_json::to_string(&cleaned).unwrap_or_default();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fresh(c: &Arc<Cache>, key: &str, method: &str, params: &Value) -> Reservation {
        match c.reserve(key, method, params).unwrap() {
            Lookup::Fresh(r) => r,
            Lookup::Replay(_) => panic!("expected a fresh key"),
            Lookup::InFlight(_) => panic!("expected a fresh key, not an in flight one"),
        }
    }

    #[test]
    fn the_same_call_twice_replays_the_first_answer() {
        let c = Cache::new();
        let params = json!({ "uri": "rtmp://h/l/k", "id": "cam1" });
        fresh(&c, "k1", "source.add", &params)
            .commit(&json!({ "id": "cam1", "should_retry": false }));

        let Lookup::Replay(body) = c.reserve("k1", "source.add", &params).unwrap() else {
            panic!("the second call has to replay");
        };
        assert_eq!(body["id"], "cam1");
        assert_eq!(body["replayed"], true, "a replay says so: {body}");
    }

    /// The Codex finding. Two requests arriving together used to both run the
    /// work, because the lookup and the store were not one step. The second
    /// now finds the key taken and waits.
    #[tokio::test]
    async fn two_callers_racing_on_one_key_do_not_both_run_the_work() {
        let c = Cache::new();
        let params = json!({ "uri": "rtmp://h/l/k" });
        let first = fresh(&c, "k1", "source.add", &params);

        let Lookup::InFlight(wake) = c.reserve("k1", "source.add", &params).unwrap() else {
            panic!("the second caller must find the key taken");
        };
        let waiting = wake.notified();
        tokio::pin!(waiting);
        // Nothing yet: the work has not finished.
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut waiting).await.is_err(),
            "a waiter must not be woken before the answer exists"
        );

        first.commit(&json!({ "id": "cam1" }));
        tokio::time::timeout(Duration::from_millis(200), waiting)
            .await
            .expect("committing wakes the waiter");
        let Lookup::Replay(body) = c.reserve("k1", "source.add", &params).unwrap() else {
            panic!("the waiter's second look has to find the answer");
        };
        assert_eq!(body["id"], "cam1");
        assert_eq!(body["replayed"], true);
    }

    /// A call that failed must not block its key for a day: the next attempt
    /// owns it again.
    #[tokio::test]
    async fn a_dropped_reservation_releases_the_key_and_wakes_the_waiters() {
        let c = Cache::new();
        let params = json!({ "uri": "rtmp://h/l/k" });
        let first = fresh(&c, "k1", "source.add", &params);
        let Lookup::InFlight(wake) = c.reserve("k1", "source.add", &params).unwrap() else {
            panic!("taken")
        };
        let waiting = wake.notified();
        tokio::pin!(waiting);
        drop(first);
        tokio::time::timeout(Duration::from_millis(200), waiting)
            .await
            .expect("a released key wakes the waiter");
        // And the key is free again.
        let _second = fresh(&c, "k1", "source.add", &params);
        assert_eq!(c.answered(), 0, "a failure is never stored");
    }

    /// A retry that re-generates its trace id is still the same call, and key
    /// order in the object is not part of the question being asked.
    #[test]
    fn a_replay_ignores_the_envelope_and_the_key_order() {
        let c = Cache::new();
        let first = json!({ "id": "cam1", "uri": "rtmp://h/l/k", "trace_id": "aaa" });
        fresh(&c, "k1", "source.add", &first).commit(&json!({ "id": "cam1" }));
        let again = json!({ "uri": "rtmp://h/l/k", "id": "cam1", "trace_id": "bbb" });
        assert!(matches!(c.reserve("k1", "source.add", &again).unwrap(), Lookup::Replay(_)));
    }

    #[test]
    fn a_reused_key_with_different_params_is_a_mismatch() {
        let c = Cache::new();
        let first = json!({ "uri": "rtmp://h/l/one" });
        fresh(&c, "k1", "source.add", &first).commit(&json!({ "id": "one" }));
        let e = c.reserve("k1", "source.add", &json!({ "uri": "rtmp://h/l/two" })).unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidParams.number());
        assert_eq!(e.data["idempotency"], "mismatch");
        assert!(e.message.contains("k1"), "{}", e.message);

        // A key reused for a different method is the same mistake, and the
        // message names the method it was first used for.
        let e = c.reserve("k1", "source.remove", &first).unwrap_err();
        assert_eq!(e.data["first_method"], "source.add");

        // The same holds while the first call is still running.
        let c = Cache::new();
        let _held = fresh(&c, "k2", "source.add", &first);
        assert!(c.reserve("k2", "source.add", &json!({ "uri": "other" })).is_err());
    }

    #[test]
    fn the_cache_does_not_grow_without_end() {
        let c = Cache::new();
        for i in 0..(MAX_KEYS + 50) {
            fresh(&c, &format!("k{i}"), "source.add", &json!({ "i": i }))
                .commit(&json!({ "ok": true }));
        }
        assert!(c.len() <= MAX_KEYS, "held {} keys", c.len());
    }
}
