//! Outputs a person stopped and has not removed.
//!
//! Until 0.3 the only way to stop sending to YouTube was `output.remove`,
//! which also forgot the address and the stream key, so starting again meant
//! finding the key in the platform's dashboard and pasting it in again. A
//! tester who could not find Remove under Outputs had no way to stop at all.
//!
//! A stopped output is kept here as it was configured, with `enabled` false.
//! Nothing is built for it: no muxer, no sink, no lease on the encoder, so a
//! mixer whose every destination is stopped encodes nothing for nobody. It is
//! in the status as `stopped`, in the runtime store with `enabled = false`,
//! and `output.start` attaches it again under the same id.

use super::Mixer;
use crate::config::OutputConfig;
use crate::state::{safe_uri_label, uri_has_key, OutputState, OutputStatus};
use anyhow::Result;
use tracing::info;

/// The stopped outputs, one entry an id, in the order they were stopped.
#[derive(Debug, Clone, Default)]
pub struct HeldList(Vec<OutputConfig>);

impl HeldList {
    /// Keep `config`, stopped, in place of any entry with its id.
    pub fn put(&mut self, config: &OutputConfig) {
        let mut config = config.clone();
        config.enabled = false;
        match self.0.iter_mut().find(|c| c.id == config.id) {
            Some(kept) => *kept = config,
            None => self.0.push(config),
        }
    }

    /// Take one out, to start it or because it was removed.
    pub fn take(&mut self, id: &str) -> Option<OutputConfig> {
        let at = self.0.iter().position(|c| c.id == id)?;
        Some(self.0.remove(at))
    }

    pub fn has(&self, id: &str) -> bool {
        self.0.iter().any(|c| c.id == id)
    }

    pub fn configs(&self) -> impl Iterator<Item = &OutputConfig> {
        self.0.iter()
    }

    /// Each as a stopped output, for the status.
    pub fn statuses(&self) -> impl Iterator<Item = OutputStatus> + '_ {
        self.0.iter().map(stopped_status)
    }
}

fn stopped_status(config: &OutputConfig) -> OutputStatus {
    let mut extra = godwinmix_protocol::types::Extra::new();
    if let Some(kind) = &config.type_id {
        extra.insert("type".into(), kind.clone().into());
    }
    OutputStatus {
        id: config.id.clone(),
        uri_host: safe_uri_label(&config.uri),
        has_key: uri_has_key(&config.uri),
        state: OutputState::Stopped,
        reconnects: 0,
        queue_secs: 0.0,
        rendition: config.rendition.clone(),
        shed: None,
        error: None,
        extra,
    }
}

impl Mixer {
    /// Stop sending to one destination and keep it. Stopping one that is
    /// already stopped is not an error: the state asked for is the state.
    pub fn stop_output(&mut self, id: &str) -> Result<()> {
        if self.held.has(id) {
            return Ok(());
        }
        let config = self
            .outputs
            .iter()
            .find(|o| o.id() == id)
            .map(|o| o.cfg.clone())
            .or_else(|| self.unattached.configs().find(|c| c.id == id).cloned());
        let Some(config) = config else {
            anyhow::bail!("no such output {id}");
        };
        // Kept first, so the runtime store the removal writes still has it.
        self.held.put(&config);
        self.remove_output(&id.to_string())?;
        info!(output = %id, "output stopped by request; its address is kept");
        Ok(())
    }

    /// Start a stopped destination again under the same id.
    pub fn start_output(&mut self, id: &str) -> Result<()> {
        if self.outputs.iter().any(|o| o.id() == id) || self.unattached.has(id) {
            return Ok(());
        }
        let Some(mut config) = self.held.take(id) else {
            anyhow::bail!("no such output {id}");
        };
        config.enabled = true;
        if let Err(e) = self.add_output(&config) {
            // Still stopped, and still kept: a refusal must not cost the key.
            self.held.put(&config);
            self.persist_runtime();
            self.broadcast_status();
            return Err(e.context(format!("output {id} is still stopped, with its address kept")));
        }
        info!(output = %id, "output started again by request");
        Ok(())
    }

    /// An output asked for already stopped, from the config or `output.add`.
    pub(super) fn hold(&mut self, config: &OutputConfig) {
        self.held.put(config);
        info!(output = %config.id, "output kept stopped, as configured");
    }
}

#[cfg(test)]
mod tests;
