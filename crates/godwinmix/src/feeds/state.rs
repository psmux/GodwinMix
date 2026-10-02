//! What a feed's task reads and reports: its plan for the next fetch, and
//! what came of the last one. The events are raised here, once per change of
//! state rather than once per attempt, so a feed that is down for an hour
//! says so once.

use super::{check, Ctx, Feeds};
use godwinmix_protocol::feeds::{FeedFormat, FeedState};
use godwinmix_protocol::types::Event;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

/// A feed's counters.
#[derive(Debug, Clone)]
pub struct Run {
    pub state: FeedState,
    pub last_fetch: Option<String>,
    pub last_change: Option<String>,
    pub last_error: Option<String>,
    pub failures: u32,
    pub fetches: u64,
    pub not_modified: u64,
    pub bytes: u64,
}

impl Default for Run {
    fn default() -> Self {
        Run {
            state: FeedState::Starting,
            last_fetch: None,
            last_change: None,
            last_error: None,
            failures: 0,
            fetches: 0,
            not_modified: 0,
            bytes: 0,
        }
    }
}

/// What one fetch needs, copied out so no lock is held across it.
pub struct Plan {
    pub address: String,
    pub headers: Vec<(String, String)>,
    pub format: FeedFormat,
    pub interval: Duration,
    pub timeout: Duration,
}

pub fn now() -> String {
    crate::channels::keys::now()
}

impl Feeds {
    /// `None` once the feed is removed or paused, which ends its task.
    pub(super) fn plan(&self, id: &str) -> Option<Plan> {
        let st = self.state.lock();
        let f = st.feeds.get(id).filter(|f| !f.spec.paused)?;
        Some(Plan {
            address: f.spec.address.clone(),
            headers: f.spec.headers.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            format: f.spec.format,
            interval: check::interval(&f.spec),
            timeout: check::timeout(&f.spec),
        })
    }

    /// The wait before the next fetch: the interval, doubled for each
    /// failure in a row up to five minutes (or the interval, if longer).
    pub(super) fn wait(&self, id: &str, interval: Duration) -> Duration {
        let failures = self.state.lock().feeds.get(id).map(|f| f.run.failures).unwrap_or(0);
        backoff(interval, failures, interval.max(Duration::from_secs(300)))
    }

    pub(super) fn failures(&self, id: &str) -> u32 {
        self.state.lock().feeds.get(id).map(|f| f.run.failures).unwrap_or(0)
    }

    pub(super) fn doc(&self, id: &str) -> Option<Arc<Value>> {
        self.state.lock().feeds.get(id).and_then(|f| f.doc.clone())
    }

    /// Something was read and it was what was there before (or a `304`).
    pub(super) fn note_same(&self, ctx: &Ctx, id: &str, not_modified: bool, bytes: Option<usize>) {
        self.note_read(ctx, id, |run| {
            if not_modified {
                run.not_modified += 1;
            }
            if let Some(n) = bytes {
                run.bytes = n as u64;
            }
        });
    }

    /// A document was read. Answers whether it differs from the last.
    pub(super) fn note_doc(&self, ctx: &Ctx, id: &str, doc: Value, bytes: usize) -> bool {
        let mut changed = false;
        let doc = Arc::new(doc);
        self.note_read(ctx, id, |run| run.bytes = bytes as u64);
        let mut st = self.state.lock();
        if let Some(f) = st.feeds.get_mut(id) {
            changed = f.doc.as_deref() != Some(&*doc);
            if changed {
                f.doc = Some(doc);
                f.run.last_change = Some(now());
            }
        }
        changed
    }

    fn note_read(&self, ctx: &Ctx, id: &str, update: impl FnOnce(&mut super::state::Run)) {
        let recovered = {
            let mut st = self.state.lock();
            let Some(f) = st.feeds.get_mut(id) else { return };
            let failures = f.run.failures;
            f.run.state = FeedState::Ok;
            f.run.failures = 0;
            f.run.last_error = None;
            f.run.fetches += 1;
            f.run.last_fetch = Some(now());
            update(&mut f.run);
            failures
        };
        if recovered > 0 {
            tracing::info!(feed = %id, after = recovered, "a feed is read again");
            ctx.app.mixer.emit(Event::FeedRecovered { id: id.into(), binding: None, failures: recovered });
        }
    }

    pub(super) fn note_failure(&self, ctx: &Ctx, id: &str, error: String) {
        let first = {
            let mut st = self.state.lock();
            let Some(f) = st.feeds.get_mut(id) else { return };
            f.run.state = FeedState::Failing;
            f.run.failures += 1;
            f.run.last_error = Some(error.clone());
            f.run.failures == 1
        };
        if first {
            tracing::warn!(feed = %id, %error, "a feed could not be read; what it last wrote stays on air");
            ctx.app.mixer.emit(Event::FeedFailed { id: id.into(), binding: None, error, failures: 1 });
        }
    }
}

/// `base` doubled `failures - 1` times, never more than `cap`.
pub fn backoff(base: Duration, failures: u32, cap: Duration) -> Duration {
    if failures == 0 {
        return base;
    }
    base.saturating_mul(1u32 << (failures - 1).min(16)).min(cap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failing_feed_waits_longer_each_time_up_to_a_cap() {
        let s = Duration::from_secs;
        assert_eq!(backoff(s(30), 0, s(300)), s(30));
        assert_eq!(backoff(s(30), 1, s(300)), s(30));
        assert_eq!(backoff(s(30), 3, s(300)), s(120));
        assert_eq!(backoff(s(30), 40, s(300)), s(300));
    }
}
