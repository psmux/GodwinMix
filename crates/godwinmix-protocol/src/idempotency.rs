//! Retries that cost nothing.
//!
//! Stripe's model, which 09 section 5 item 4 asks for: a mutating call carries
//! an `idempotency_key`, the core remembers the answer for 24 hours, and a
//! repeat gets the first answer back with `replayed: true`. A repeat under the
//! same key with different params is `-32602` with `data.idempotency:
//! "mismatch"`, because the alternative is silently answering a question
//! nobody asked.
//!
//! In memory, per process. A key that survived a restart would be answering
//! for a mixer that no longer holds the state the answer describes.

use crate::error::{ErrorCode, RpcError};
use serde_json::Value;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long an answer is worth replaying. The number in 03 section 6.
pub const TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Keys held at once before the oldest are dropped early. A key is small; a
/// client looping on failure is not, and an unbounded map in a process that
/// holds a live programme is a leak with a deadline.
const MAX_KEYS: usize = 4096;

#[derive(Debug, Clone)]
struct Entry {
    method: String,
    params_hash: u64,
    body: Value,
    at: Instant,
}

#[derive(Debug, Default)]
pub struct Cache {
    entries: parking_lot::Mutex<HashMap<String, Entry>>,
}

/// What a lookup found.
#[derive(Debug)]
pub enum Lookup {
    /// Nothing under this key. Go and do the work, then `store`.
    Fresh,
    /// The same call, already answered. Hand this body back.
    Replay(Value),
}

impl Cache {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Look a key up. The params are hashed rather than kept, because a body
    /// carrying a stream key should not sit in memory for a day.
    pub fn lookup(&self, key: &str, method: &str, params: &Value) -> Result<Lookup, RpcError> {
        let mut entries = self.entries.lock();
        prune(&mut entries);
        let Some(entry) = entries.get(key) else {
            return Ok(Lookup::Fresh);
        };
        if entry.method != method {
            return Err(mismatch(key, &entry.method, method));
        }
        if entry.params_hash != hash_params(params) {
            return Err(mismatch(key, method, method).with("params", "differ"));
        }
        let mut body = entry.body.clone();
        if let Some(map) = body.as_object_mut() {
            map.insert("replayed".into(), Value::Bool(true));
        }
        Ok(Lookup::Replay(body))
    }

    /// Remember an answer. Only successful answers are stored: a failure that
    /// the caller could fix and retry must not be frozen for a day.
    pub fn store(&self, key: &str, method: &str, params: &Value, body: &Value) {
        let mut entries = self.entries.lock();
        prune(&mut entries);
        if entries.len() >= MAX_KEYS {
            drop_oldest(&mut entries);
        }
        entries.insert(
            key.to_string(),
            Entry {
                method: method.to_string(),
                params_hash: hash_params(params),
                body: body.clone(),
                at: Instant::now(),
            },
        );
    }

    pub fn len(&self) -> usize {
        self.entries.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn prune(entries: &mut HashMap<String, Entry>) {
    entries.retain(|_, e| e.at.elapsed() < TTL);
}

fn drop_oldest(entries: &mut HashMap<String, Entry>) {
    if let Some(key) = entries
        .iter()
        .max_by_key(|(_, e)| e.at.elapsed())
        .map(|(k, _)| k.clone())
    {
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

    #[test]
    fn the_same_call_twice_replays_the_first_answer() {
        let c = Cache::default();
        let params = json!({ "uri": "rtmp://h/l/k", "id": "cam1" });
        assert!(matches!(
            c.lookup("k1", "source.add", &params).unwrap(),
            Lookup::Fresh
        ));
        c.store(
            "k1",
            "source.add",
            &params,
            &json!({ "id": "cam1", "should_retry": false }),
        );

        let Lookup::Replay(body) = c.lookup("k1", "source.add", &params).unwrap() else {
            panic!("the second call has to replay");
        };
        assert_eq!(body["id"], "cam1");
        assert_eq!(body["replayed"], true, "a replay says so: {body}");
    }

    /// A retry that re-generates its trace id is still the same call, and key
    /// order in the object is not part of the question being asked.
    #[test]
    fn a_replay_ignores_the_envelope_and_the_key_order() {
        let c = Cache::default();
        let first = json!({ "id": "cam1", "uri": "rtmp://h/l/k", "trace_id": "aaa" });
        c.store("k1", "source.add", &first, &json!({ "id": "cam1" }));
        let again = json!({ "uri": "rtmp://h/l/k", "id": "cam1", "trace_id": "bbb" });
        assert!(matches!(
            c.lookup("k1", "source.add", &again).unwrap(),
            Lookup::Replay(_)
        ));
    }

    #[test]
    fn a_reused_key_with_different_params_is_a_mismatch() {
        let c = Cache::default();
        let first = json!({ "uri": "rtmp://h/l/one" });
        c.store("k1", "source.add", &first, &json!({ "id": "one" }));
        let e = c
            .lookup("k1", "source.add", &json!({ "uri": "rtmp://h/l/two" }))
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidParams.number());
        assert_eq!(e.data["idempotency"], "mismatch");
        assert!(e.message.contains("k1"), "{}", e.message);

        // A key reused for a different method is the same mistake, and the
        // message names the method it was first used for.
        let e = c.lookup("k1", "source.remove", &first).unwrap_err();
        assert_eq!(e.data["first_method"], "source.add");
    }

    #[test]
    fn the_cache_does_not_grow_without_end() {
        let c = Cache::default();
        for i in 0..(MAX_KEYS + 50) {
            c.store(
                &format!("k{i}"),
                "source.add",
                &json!({ "i": i }),
                &json!({ "ok": true }),
            );
        }
        assert!(c.len() <= MAX_KEYS, "held {} keys", c.len());
    }
}
