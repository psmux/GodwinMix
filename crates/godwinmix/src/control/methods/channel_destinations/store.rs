//! The one seam between these methods and whoever keeps channels.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use godwinmix_protocol::destination::{DestinationLive, StoredDestination};
use godwinmix_protocol::error::RpcError;
use serde_json::{json, Value};

/// An edit to one channel's destinations. It may refuse, and a refusal
/// leaves the list as it was.
pub type Edit<'a> = &'a mut dyn FnMut(&mut Vec<StoredDestination>) -> Result<(), RpcError>;

/// Where a channel's destinations are kept.
///
/// `edit_destinations` finds the channel (or answers
/// `RpcError::not_found("channel", id, &ids)`), runs `edit` on its list under
/// its own lock, and when the list came out different: persists it, starts,
/// restarts or stops the restreamer for each destination that changed, sends
/// `event/channel.changed`, and answers the channel as a client sees it, with
/// each destination through `StoredDestination::view`. An edit that leaves
/// the list as it was saves nothing and sends nothing.
pub trait ChannelStore: Send + Sync {
    fn edit_destinations(&self, channel: &str, edit: Edit) -> Result<Value, RpcError>;
}

/// The store the methods use until the channel server is merged: channels
/// held in memory, none of them there until a test puts one in.
#[derive(Default)]
pub struct MemoryStore {
    channels: Mutex<BTreeMap<String, Vec<StoredDestination>>>,
}

impl MemoryStore {
    /// Make a channel with no destinations. Only tests have a reason to.
    #[cfg(test)]
    pub fn open(&self, channel: &str) {
        self.channels.lock().unwrap_or_else(|e| e.into_inner()).entry(channel.into()).or_default();
    }
}

impl ChannelStore for MemoryStore {
    fn edit_destinations(&self, channel: &str, edit: Edit) -> Result<Value, RpcError> {
        let mut channels = self.channels.lock().unwrap_or_else(|e| e.into_inner());
        let ids: Vec<String> = channels.keys().cloned().collect();
        let list = channels.get_mut(channel).ok_or_else(|| RpcError::not_found("channel", channel, &ids))?;
        let mut wanted = list.clone();
        edit(&mut wanted)?;
        *list = wanted;
        let views: Vec<_> = list.iter().map(|d| d.view(DestinationLive::default())).collect();
        Ok(json!({ "id": channel, "destinations": views }))
    }
}

/// The store in force for this process. At the merge this returns the
/// channel server's store and `memory` goes.
pub fn current() -> Arc<dyn ChannelStore> {
    memory()
}

fn memory() -> Arc<MemoryStore> {
    static STORE: OnceLock<Arc<MemoryStore>> = OnceLock::new();
    STORE.get_or_init(Default::default).clone()
}

/// Open a channel in the memory store, for a test.
#[cfg(test)]
pub fn open_for_test(channel: &str) {
    memory().open(channel)
}
