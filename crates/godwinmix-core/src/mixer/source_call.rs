//! A tool call addressed to one source, answered off the mixer thread.
//!
//! A plugin's `[[tools]]` used to reach only its singleton instances: a
//! service, a device. A source plugin runs one process per source, and none of
//! those processes was reachable by name, so a playlist plugin with three
//! playlists on air had no way to be told "skip" on the second one. This asks
//! the mixer for the pipeline behind a source id, which is one lookup on the
//! mixer thread, and then sends `tool.call` to the plugin behind it from a
//! blocking task, where waiting up to the protocol's five seconds holds up
//! nothing but the caller.

use super::{Command, MixerHandle};
use crate::state::SourceId;
use anyhow::Result;
use serde_json::{json, Value};
use tokio::sync::oneshot;

/// What `source_tool_call` found.
#[derive(Debug)]
pub enum SourceToolOutcome {
    /// The plugin answered, in MCP's shape.
    Answered(Value),
    NoSuchSource,
    /// The source exists and is not a plugin of that name. Carries the
    /// plugin it is, or the built in kind, so the refusal can name it.
    OtherPlugin(String),
    /// The plugin refused, or did not answer.
    Failed(String),
}

impl MixerHandle {
    /// Send `tool.call {name: tool, arguments}` to the plugin behind
    /// `source`. `plugin` is checked against the source's own plugin, so a
    /// tool name meant for one plugin never lands in another's process.
    pub async fn source_tool_call(
        &self,
        source: SourceId,
        plugin: Option<String>,
        tool: String,
        arguments: Value,
    ) -> Result<SourceToolOutcome> {
        let (tx, rx) = oneshot::channel();
        let Some(input) = self.ask(Command::SourceInput { source, reply: tx }, rx).await? else {
            return Ok(SourceToolOutcome::NoSuchSource);
        };
        let owner = input.manifest().plugin.to_string();
        if plugin.as_deref().is_some_and(|p| p != owner) {
            return Ok(SourceToolOutcome::OtherPlugin(owner));
        }
        let answered = tokio::task::spawn_blocking(move || {
            input.call("tool.call", json!({ "name": tool, "arguments": arguments }))
        })
        .await
        .map_err(|e| anyhow::anyhow!("the tool call panicked: {e}"))?;
        Ok(match answered {
            Ok(value) => SourceToolOutcome::Answered(value),
            Err(e) => SourceToolOutcome::Failed(format!("{e:#}")),
        })
    }
}
