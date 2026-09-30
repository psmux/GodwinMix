//! Reading a channel's key back, for the admin who has to give it out.
//!
//! The keys are sealed in the secret store rather than thrown away, because
//! the listener needs them to let publishers in. Reading one back is an admin
//! call, one key at a time, and every read is logged with who asked. The log
//! names the key, never its value.

use godwinmix_protocol::channels::{ChannelKeyRevealRequest, KeyRevealed};
use godwinmix_protocol::error::RpcError;
use tracing::info;

use super::keys;
use super::Channels;

impl Channels {
    /// `channel.key.reveal`. `caller` is the token id, for the log.
    pub fn key_reveal(&self, req: ChannelKeyRevealRequest, caller: &str) -> Result<KeyRevealed, RpcError> {
        let known: Vec<String> = {
            let records = self.records.lock();
            let record = records.iter().find(|r| r.id == req.id).ok_or_else(|| self.missing(&req.id, &records))?;
            record.keys.iter().map(|k| k.id.clone()).collect()
        };
        if !known.contains(&req.key) {
            return Err(RpcError::not_found("key", &req.key, &known).with("channel", req.id.clone()));
        }
        let secret = self
            .secrets
            .get(&keys::scope(&req.id), &req.key)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| unsealed(&req.id, &req.key))?;
        info!(channel = %req.id, key = %req.key, caller, "channel key revealed");
        Ok(KeyRevealed { secret })
    }

    fn missing(&self, id: &str, records: &[super::Record]) -> RpcError {
        let ids: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
        RpcError::not_found("channel", id, &ids)
    }
}

/// The record is there but the secret store has nothing for it: the store
/// was deleted or its key file replaced. Nothing can bring it back.
fn unsealed(channel: &str, key: &str) -> RpcError {
    RpcError::not_in_state(format!(
        "the key '{key}' of channel '{channel}' is not in the secret store any more, so it \
         cannot be shown, and no encoder can publish with it. Take it back and make a new \
         key in its place."
    ))
    .with("channel", channel)
    .with("key", key)
}
