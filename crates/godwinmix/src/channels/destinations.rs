//! A channel's destinations, kept with the channel.
//!
//! The record on disk says what a list shows (platform, label, the host, which
//! stream, on or off). The address and the key are sealed in the secret
//! store under `channel.<id>.destination`, beside the channel's own keys, so
//! removing the channel forgets them with the rest. The listener is handed
//! the whole address in its table and runs one restream per destination that
//! is on; what it reports comes back through `sending.rs`.

use godwinmix_protocol::destination::{uri_host, Destination, DestinationState, StoredDestination};
use godwinmix_protocol::error::RpcError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::keys;
use super::store::{DestinationRecord, Record};
use super::Channels;
use crate::control::methods::channel_destinations::{ChannelStore, Edit};

/// Where a channel's destination addresses and keys are sealed.
fn scope(channel: &str) -> String {
    format!("{}.destination", keys::scope(channel))
}

/// What is sealed for one destination.
#[derive(Default, Serialize, Deserialize)]
struct Sealed {
    server: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    key: Option<String>,
}

impl Channels {
    /// A channel's destinations with their addresses and keys unsealed.
    pub(super) fn stored(&self, r: &Record) -> Vec<StoredDestination> {
        r.destinations
            .iter()
            .map(|d| {
                let sealed: Sealed = self
                    .secrets
                    .get(&scope(&r.id), &d.id)
                    .and_then(|text| serde_json::from_str(&text).ok())
                    .unwrap_or_default();
                StoredDestination {
                    id: d.id.clone(),
                    platform: d.platform.clone(),
                    label: d.label.clone(),
                    server: sealed.server,
                    key: sealed.key,
                    stream: d.stream.clone(),
                    enabled: d.enabled,
                    rendition: d.rendition.clone(),
                }
            })
            .collect()
    }

    /// Seal what changed, forget what went, and answer the records to keep.
    fn seal(
        &self,
        channel: &str,
        before: &[StoredDestination],
        after: &[StoredDestination],
    ) -> Result<Vec<DestinationRecord>, RpcError> {
        let scope = scope(channel);
        for gone in before.iter().filter(|b| !after.iter().any(|a| a.id == b.id)) {
            let _ = self.secrets.set(&scope, &gone.id, "");
        }
        let mut records = Vec::with_capacity(after.len());
        for d in after {
            let was = before.iter().find(|b| b.id == d.id);
            if was.is_none_or(|b| b.server != d.server || b.key != d.key) {
                let text = serde_json::to_string(&Sealed { server: d.server.clone(), key: d.key.clone() })
                    .map_err(|e| RpcError::internal(format!("sealing a destination: {e}")))?;
                self.secrets
                    .set(&scope, &d.id, &text)
                    .map_err(|e| RpcError::internal(format!("sealing a destination: {e:#}")))?;
            }
            records.push(DestinationRecord {
                id: d.id.clone(),
                platform: d.platform.clone(),
                label: d.label.clone(),
                uri_host: uri_host(&d.server),
                has_key: d.has_key(),
                stream: d.stream.clone(),
                enabled: d.enabled,
                rendition: d.rendition.clone(),
            });
        }
        Ok(records)
    }

    /// What the listener is handed for one channel: each destination that is
    /// on, with its whole address. One that asked for a rendition carries
    /// what the plan gave it (`transcode::Transcode::row`); one the plan
    /// copies, or that asked for nothing, is the same row as ever.
    pub(super) fn destination_table(&self, r: &Record) -> Vec<Value> {
        self.stored(r)
            .iter()
            .filter(|d| d.enabled)
            .filter_map(|d| {
                let row = json!({"id": d.id, "platform": d.platform, "url": d.url(), "stream": d.stream});
                self.transcode.row(&r.id, d, row)
            })
            .collect()
    }

    /// A channel's destinations as a client sees them, each with what the
    /// listener last said about it, and what its rendition was planned to.
    pub(super) fn destination_views(&self, r: &Record) -> Vec<Destination> {
        r.destinations
            .iter()
            .map(|d| {
                let mut live = self.sending_view(&r.id, &d.id, d.enabled);
                let (plan, refused) = match (&d.rendition, d.enabled) {
                    (Some(_), true) => self.transcode.view(&r.id, &d.id),
                    _ => (None, None),
                };
                if let Some(no) = &refused {
                    live.state = DestinationState::Failed;
                    live.error = Some(no.message.clone());
                    live.kbps = 0;
                }
                Destination {
                    id: d.id.clone(),
                    platform: d.platform.clone(),
                    label: d.label.clone(),
                    uri_host: d.uri_host.clone(),
                    has_key: d.has_key,
                    stream: d.stream.clone(),
                    enabled: d.enabled,
                    live,
                    rendition: d.rendition.clone(),
                    plan,
                    refused,
                }
            })
            .collect()
    }
}

impl Channels {
    /// Seal, keep, persist and hand over a channel's new destination list.
    fn keep_destinations(&self, channel: &str, before: &[StoredDestination], after: &[StoredDestination]) -> Result<(), RpcError> {
        let kept = self.seal(channel, before, after)?;
        if let Some(r) = self.records.lock().iter_mut().find(|r| r.id == channel) {
            r.destinations = kept;
        }
        self.forget_sending(channel, before, after);
        self.commit(Some(channel))
    }
}

impl ChannelStore for Channels {
    fn edit_destinations(&self, channel: &str, edit: Edit) -> Result<Value, RpcError> {
        let _one_at_a_time = self.edits.lock();
        let record = self.records.lock().iter().find(|r| r.id == channel).cloned();
        let Some(record) = record else { return Err(self.not_found(channel)) };
        let before = self.stored(&record);
        let mut after = before.clone();
        edit(&mut after)?;
        if after != before {
            self.keep_destinations(channel, &before, &after)?;
            // The governor had no room for a rendition this edit asked for:
            // the edit is undone and the refusal, with what would fit, is
            // the answer, as it is for a programme output.
            let touched = after.iter().filter(|d| !before.contains(d));
            if let Some(no) = touched.filter_map(|d| self.transcode.refusal_error(channel, d)).next() {
                self.keep_destinations(channel, &after, &before)?;
                return Err(no);
            }
        }
        let answer = self.get(channel)?;
        serde_json::to_value(answer).map_err(|e| RpcError::internal(format!("the channel would not serialise: {e}")))
    }
}
