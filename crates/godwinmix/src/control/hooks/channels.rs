//! The channels' hooks, `channel.stream.state` and
//! `channel.destination.state`, fired through the same [`Hooks`] as every
//! other. The channels raise them on a thread of their own, outside any
//! runtime, so the runtime a hook's task is spawned on is kept here.

use std::sync::Arc;

use serde_json::Value;

use super::Hooks;

pub struct ForChannels {
    hooks: Arc<Hooks>,
    runtime: tokio::runtime::Handle,
}

impl ForChannels {
    /// `None` outside a Tokio runtime, where no hook could run.
    pub fn new(hooks: &Arc<Hooks>) -> Option<Arc<ForChannels>> {
        let runtime = tokio::runtime::Handle::try_current().ok()?;
        Some(Arc::new(ForChannels { hooks: hooks.clone(), runtime }))
    }
}

impl crate::channels::hooks::Hook for ForChannels {
    fn wants(&self, event: &str) -> bool {
        self.hooks.any(event)
    }

    fn fire(&self, event: &'static str, payload: Value) {
        let _inside = self.runtime.enter();
        self.hooks.fire(event, || payload);
    }
}
