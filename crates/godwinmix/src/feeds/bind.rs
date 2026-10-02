//! A feed's document through each of its bindings, and a write only where
//! the value is not what was last written.
//!
//! That comparison is the whole of "write only on change": an unchanged feed
//! never gets here (its body is the same, or the server said `304`), and a
//! changed feed whose bound values are the same writes nothing.

use super::{value, write, Ctx, Feeds};
use godwinmix_protocol::feeds::{BindingTarget, Selection};
use godwinmix_protocol::types::Event;
use serde_json::Value;
use std::sync::Arc;

struct Job {
    id: String,
    selection: Selection,
    to: BindingTarget,
    written: Option<Value>,
}

/// Run `feed`'s bindings over its document. With `only_failing`, just the
/// ones whose last write failed, so a source that has come back gets its
/// value without waiting for the feed to change.
pub async fn apply(feeds: &Arc<Feeds>, ctx: &Ctx, feed: &str, only_failing: bool) {
    let Some(doc) = feeds.doc(feed) else { return };
    for job in feeds.jobs(feed, None, only_failing) {
        one(feeds, ctx, &doc, job).await;
    }
}

/// One binding, now, whatever was written before.
pub async fn force(feeds: &Arc<Feeds>, ctx: &Ctx, binding: &str) {
    let Some(feed) = feeds.state.lock().bindings.get(binding).map(|b| b.spec.feed.clone()) else { return };
    let Some(doc) = feeds.doc(&feed) else { return };
    for mut job in feeds.jobs(&feed, Some(binding), false) {
        job.written = None;
        one(feeds, ctx, &doc, job).await;
    }
}

async fn one(feeds: &Arc<Feeds>, ctx: &Ctx, doc: &Value, job: Job) {
    let result = match value::compute(doc, &job.selection) {
        Err(e) => Err(e.message),
        Ok((_, out)) if Some(&out) == job.written.as_ref() => Ok(None),
        Ok((_, out)) => write::to(ctx, &job.to, &out).await.map(|_| Some(out)),
    };
    feeds.binding_result(ctx, &job.id, result);
}

impl Feeds {
    fn jobs(&self, feed: &str, only: Option<&str>, only_failing: bool) -> Vec<Job> {
        let st = self.state.lock();
        st.bindings
            .values()
            .filter(|b| b.spec.feed == feed && !b.spec.paused)
            .filter(|b| only.is_none_or(|id| b.spec.id == id))
            .filter(|b| !only_failing || b.failures > 0)
            .map(|b| Job { id: b.spec.id.clone(), selection: b.spec.selection(), to: b.spec.to.clone(), written: b.written.clone() })
            .collect()
    }

    pub(super) fn any_failing(&self, feed: &str) -> bool {
        self.state.lock().bindings.values().any(|b| b.spec.feed == feed && b.failures > 0 && !b.spec.paused)
    }

    fn binding_result(&self, ctx: &Ctx, id: &str, result: Result<Option<Value>, String>) {
        let (feed, event) = {
            let mut st = self.state.lock();
            let Some(b) = st.bindings.get_mut(id) else { return };
            let before = b.failures;
            let event = match result {
                Ok(wrote) => {
                    if let Some(v) = wrote {
                        b.written = Some(v);
                        b.writes += 1;
                        b.last_write = Some(super::state::now());
                    }
                    b.failures = 0;
                    b.last_error = None;
                    (before > 0).then_some(Err(before))
                }
                Err(e) => {
                    b.failures += 1;
                    b.last_error = Some(e.clone());
                    (before == 0).then_some(Ok(e))
                }
            };
            (b.spec.feed.clone(), event)
        };
        match event {
            Some(Ok(error)) => {
                tracing::warn!(%feed, binding = %id, %error, "a feed binding could not write; what it last wrote stays on air");
                ctx.app.mixer.emit(Event::FeedFailed { id: feed, binding: Some(id.into()), error, failures: 1 });
            }
            Some(Err(failures)) => ctx.app.mixer.emit(Event::FeedRecovered { id: feed, binding: Some(id.into()), failures }),
            None => {}
        }
    }
}
