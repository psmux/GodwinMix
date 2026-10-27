//! A show removed and made again with the same id and input, as a headend
//! remakes a channel, may never be gone as the host sees it: tables come
//! faster than it takes them, its row comes back the same, and its input
//! runs on and says nothing new. So what the host said of a removed show is
//! kept a minute, and given back to a show made again on the same input.

use std::time::{Duration, Instant};

use super::Direct;
use crate::station::registry::Record;
use godwinmix_protocol::shows::InputSpec;

/// How long a removed show's input is remembered for a show made again.
const REMADE: Duration = Duration::from_secs(60);

impl Direct {
    /// Forget what the host said about a show that went, but keep it a
    /// minute with the input the show had. A show removed and made again
    /// with the same id and input (a headend remaking a channel) may never
    /// be gone as the host sees it: tables come faster than it takes them,
    /// its row comes back the same, its input runs on and says nothing new.
    /// Dropped for good, the channel waited for ever for an input that was
    /// already live.
    pub fn forget(&self, id: &str, input: Option<&InputSpec>) {
        let Some(seen) = self.seen.lock().remove(id) else { return };
        let key = serde_json::to_value(input).unwrap_or_default();
        self.gone.lock().insert(id.to_string(), (key, seen, Instant::now()));
    }

    /// Give a show made again what its input said before, when the host has
    /// said nothing of it since. Whatever the host says next is taken over it.
    pub(super) fn settle_gone(&self, records: &[Record]) {
        let mut gone = self.gone.lock();
        gone.retain(|_, (_, _, at)| at.elapsed() < REMADE);
        let mut seen = self.seen.lock();
        for r in records {
            let same = |key: &serde_json::Value| *key == serde_json::to_value(&r.input).unwrap_or_default();
            let Some((_, was, _)) = gone.get(&r.id).filter(|(key, _, _)| same(key)) else { continue };
            match seen.get_mut(&r.id) {
                None => {
                    seen.insert(r.id.clone(), was.clone());
                }
                Some(now) if now.input.is_none() => now.input = was.input.clone(),
                Some(_) => {}
            }
            gone.remove(&r.id);
        }
    }
}
