//! What `feed.list` and every `feed.*` answer: the specs with header values
//! hidden, and what each feed and binding has been doing.

use super::{check, store, Binding, Feed, Feeds};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::feeds::{BindingStatus, FeedList, FeedStatus};

impl Feeds {
    pub fn list(&self) -> FeedList {
        let st = self.state.lock();
        FeedList {
            feeds: st.feeds.values().map(feed_status).collect(),
            bindings: st.bindings.values().map(binding_status).collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.state.lock().feeds.is_empty()
    }

    pub fn status(&self, id: &str) -> Result<FeedStatus, RpcError> {
        let st = self.state.lock();
        match st.feeds.get(id) {
            Some(f) => Ok(feed_status(f)),
            None => Err(RpcError::not_found("feed", id, &st.feeds.keys().cloned().collect::<Vec<_>>())),
        }
    }

    pub fn binding_status(&self, id: &str) -> Result<BindingStatus, RpcError> {
        let st = self.state.lock();
        match st.bindings.get(id) {
            Some(b) => Ok(binding_status(b)),
            None => Err(RpcError::not_found("binding", id, &st.bindings.keys().cloned().collect::<Vec<_>>())),
        }
    }
}

fn feed_status(f: &Feed) -> FeedStatus {
    FeedStatus {
        spec: store::hidden(&f.spec),
        state: f.run.state,
        kind: check::kind(&f.spec).name().into(),
        last_fetch: f.run.last_fetch.clone(),
        last_change: f.run.last_change.clone(),
        last_error: f.run.last_error.clone(),
        failures: f.run.failures,
        fetches: f.run.fetches,
        not_modified: f.run.not_modified,
        bytes: f.run.bytes,
    }
}

fn binding_status(b: &Binding) -> BindingStatus {
    BindingStatus {
        spec: b.spec.clone(),
        value: b.written.clone(),
        last_write: b.last_write.clone(),
        writes: b.writes,
        last_error: b.last_error.clone(),
    }
}
