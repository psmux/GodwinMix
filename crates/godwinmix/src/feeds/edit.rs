//! `feed.add`, `feed.set`, `feed.pause`, `feed.refresh` and `feed.remove`.

use super::{check, Ctx, Feed, Feeds};
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::feeds::{FeedSetRequest, FeedSpec, FeedState, FeedStatus};
use std::sync::Arc;

impl Feeds {
    pub fn add(self: &Arc<Self>, ctx: &Ctx, mut spec: FeedSpec) -> Result<FeedStatus, RpcError> {
        check::feed(&spec)?;
        {
            let mut st = self.state.lock();
            if st.feeds.contains_key(&spec.id) {
                return Err(RpcError::new(
                    ErrorCode::InvalidParams,
                    format!("there is a feed called '{}' already. Change it with feed.set, or give this one another id.", spec.id),
                )
                .with("field", "id")
                .with("id", spec.id.clone()));
            }
            self.seal(&mut spec, None);
            let mut feed = Feed::new(spec.clone());
            if spec.paused {
                feed.run.state = FeedState::Paused;
            }
            st.feeds.insert(spec.id.clone(), feed);
            self.save(&st);
        }
        tracing::info!(feed = %spec.id, kind = check::kind(&spec).name(), "a feed was added");
        self.spawn(ctx, &spec.id);
        self.status(&spec.id)
    }

    pub fn set(self: &Arc<Self>, ctx: &Ctx, req: FeedSetRequest) -> Result<FeedStatus, RpcError> {
        let before = self.spec(&req.id)?;
        let mut spec = before.clone();
        if let Some(a) = req.address {
            spec.address = a;
        }
        if let Some(f) = req.format {
            spec.format = f;
        }
        if req.interval_s.is_some() {
            spec.interval_s = req.interval_s;
        }
        if req.timeout_s.is_some() {
            spec.timeout_s = req.timeout_s;
        }
        if let Some(h) = req.headers {
            spec.headers = h;
        }
        check::feed(&spec)?;
        self.seal(&mut spec, Some(&before));
        for gone in before.headers.keys().filter(|k| !spec.headers.contains_key(*k)) {
            let _ = crate::control::methods::plugins::secrets().set(&format!("{}.{}", self.scope, spec.id), gone, "");
        }
        {
            let mut st = self.state.lock();
            if let Some(f) = st.feeds.get_mut(&spec.id) {
                f.spec = spec.clone();
                f.run = Default::default();
            }
            self.save(&st);
        }
        self.spawn(ctx, &spec.id);
        self.status(&spec.id)
    }

    pub fn pause(self: &Arc<Self>, ctx: &Ctx, id: &str, paused: bool) -> Result<FeedStatus, RpcError> {
        self.spec(id)?;
        {
            let mut st = self.state.lock();
            if let Some(f) = st.feeds.get_mut(id) {
                f.spec.paused = paused;
                f.run.state = if paused { FeedState::Paused } else { FeedState::Starting };
            }
            self.save(&st);
        }
        self.spawn(ctx, id);
        self.status(id)
    }

    /// Fetch now rather than at the end of the interval.
    pub fn refresh(&self, id: &str) -> Result<FeedStatus, RpcError> {
        let spec = self.spec(id)?;
        if spec.paused {
            return Err(RpcError::not_in_state(format!("the feed '{id}' is paused, so it fetches nothing. Start it with feed.pause and paused: false."))
                .with("id", id)
                .with("state", "paused"));
        }
        if let Some(f) = self.state.lock().feeds.get(id) {
            f.wake.notify_one();
        }
        self.status(id)
    }

    /// The feed and every binding that reads it.
    pub fn remove(&self, id: &str) -> Result<Vec<String>, RpcError> {
        self.spec(id)?;
        let gone = {
            let mut st = self.state.lock();
            if let Some(mut f) = st.feeds.remove(id) {
                if let Some(task) = f.task.take() {
                    task.abort();
                }
            }
            let gone: Vec<String> = st.bindings.values().filter(|b| b.spec.feed == id).map(|b| b.spec.id.clone()).collect();
            for b in &gone {
                st.bindings.remove(b);
            }
            self.save(&st);
            gone
        };
        if self.file.is_some() {
            self.forget_secrets(id);
        }
        Ok(gone)
    }

    /// The spec with its real headers, or the refusal that lists the ids.
    pub(super) fn spec(&self, id: &str) -> Result<FeedSpec, RpcError> {
        let st = self.state.lock();
        match st.feeds.get(id) {
            Some(f) => Ok(f.spec.clone()),
            None => Err(RpcError::not_found("feed", id, &st.feeds.keys().cloned().collect::<Vec<_>>())),
        }
    }
}
