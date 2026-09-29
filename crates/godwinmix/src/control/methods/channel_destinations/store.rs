//! The one seam between these methods and whoever keeps channels.

#[cfg(test)]
use std::collections::BTreeMap;
#[cfg(test)]
use std::sync::Mutex;

use godwinmix_protocol::destination::StoredDestination;
#[cfg(test)]
use godwinmix_protocol::destination::DestinationLive;
use godwinmix_protocol::error::RpcError;
use serde_json::Value;
#[cfg(test)]
use serde_json::json;

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

/// Channels held in memory, for the methods' own tests.
#[cfg(test)]
#[derive(Default)]
pub struct MemoryStore {
    channels: Mutex<BTreeMap<String, Vec<StoredDestination>>>,
}

#[cfg(test)]
impl MemoryStore {
    pub fn open(&self, channel: &str) {
        self.channels.lock().unwrap_or_else(|e| e.into_inner()).entry(channel.into()).or_default();
    }
}

#[cfg(test)]
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
