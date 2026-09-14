//! `SidecarService` and `SidecarDevice`: the two kinds with no media.
//!
//! A service is control plane only. It gets `GMX_RPC`, which is the WebSocket
//! URL of the core's own `/rpc`, and a token scoped to itself, and it calls
//! core methods like any other client. An AI director, a scheduler, a tally
//! sender and an OSC bridge are all this kind, and none of them needs a frame.
//!
//! A device discovers things the core could add. It answers `discover` with
//! candidates whose `params` are ready for `source.add`, so an operator picking
//! "CAM 1 (Studio)" off a list never types an address.
//!
//! Both are singletons named after their provide, as 03 section 4 says: there
//! is one mDNS browser per plugin, not one per source.

use super::process::{Notice, Sidecar};
use super::source::{canvas_of, params_json, state_of, SidecarSpec};
use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::plugin::{Configure, Health, PluginState};
use anyhow::{Context, Result};
use godwinmix_protocol::plugin::wire::{Candidate, Discovered, HealthState, InstanceState};
use serde_json::{json, Value};
use std::time::Duration;
use tracing::debug;

/// How long a `discover` call may take. Longer than an ordinary call, because
/// an mDNS browse is a wait by nature, and still inside the protocol's five
/// second ceiling: anything slower answers with a task id instead.
pub const DISCOVER_TIMEOUT: Duration = Duration::from_millis(4500);

/// A plugin process with no media at all.
pub struct SidecarService {
    spec: SidecarSpec,
    child: Option<Sidecar>,
    instance: String,
}

impl SidecarService {
    pub fn new(spec: SidecarSpec) -> Self {
        let instance = format!("{}-{}", spec.plugin.plugin.name, spec.provide);
        Self { spec, child: None, instance }
    }

    pub fn instance(&self) -> &str {
        &self.instance
    }

    pub fn provide_id(&self) -> String {
        self.spec.manifest.provide_id()
    }

    pub fn instance_state(&self) -> InstanceState {
        self.child.as_ref().map(Sidecar::state).unwrap_or(InstanceState::Stopped)
    }

    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().and_then(Sidecar::pid)
    }

    pub fn notices(&self) -> Vec<Notice> {
        self.child.as_ref().map(Sidecar::drain).unwrap_or_default()
    }

    pub fn configure_log(&self, level: &str) -> Result<()> {
        self.child
            .as_ref()
            .context("the plugin is not running, so there is nothing to tell")?
            .configure_log(level)
    }

    /// Start the process and shake hands. A service declares no transport and
    /// is given none: asking for one is a manifest that says it is a service
    /// and behaves like a source, and the error says so.
    pub fn start(&mut self, canvas: &CanvasCaps, params: &Params) -> Result<()> {
        self.spec.canvas = canvas.clone();
        let plugin = self.spec.plugin.plugin.name.clone();
        let provide = self.spec.provide.clone();
        let mut child = Sidecar::spawn(&self.instance, &self.spec.launch)
            .with_context(|| format!("starting `{}`", self.spec.launch.command_line()))?;
        let outcome = child.handshake_for(
            Some(&self.spec.plugin),
            canvas_of(canvas),
            &self.spec.provide,
            params_json(params),
            false,
            |t| {
                anyhow::bail!(
                    "a {} provide carries no media, so there is no `{}` address to give it. \
                     Drop `transports` from the provide, or declare it as a source.",
                    self.spec.provide,
                    t.as_str()
                )
            },
        );
        if let Err(e) = outcome {
            child.lifecycle_mut().failed(e.to_string());
            child.shutdown("the handshake failed");
            return Err(e);
        }
        // The same three the source path writes, so a singleton is in
        // `plugin.list`, in `plugin.stats` and under the budget sampler
        // exactly as a source instance is. Without them a service is a process
        // nothing can see and nothing will hold to a limit.
        crate::plugin::loader::set_pid(&self.instance, &plugin, &provide, child.pid());
        crate::plugin::loader::set_state(&self.instance, "ready");
        self.child = Some(child);
        Ok(())
    }

    pub fn stop(&mut self, reason: &str) {
        // Only when there was something running. A `SidecarService` that has
        // already been stopped is dropped later, and clearing the rows again
        // would wipe whatever has taken its instance name since: a reload
        // builds the replacement under the same name, and the old value's own
        // `Drop` used to take the new one's pid out from under it.
        let Some(child) = self.child.as_mut() else { return };
        child.shutdown(reason);
        self.child = None;
        crate::plugin::loader::set_pid(&self.instance, "", "", None);
        crate::plugin::loader::set_state(&self.instance, "stopped");
    }

    /// One call on the control channel, whatever the method is.
    ///
    /// The state machine decides what is legal, not this: `Sidecar::call`
    /// refuses a method the instance is not in a state for and names the state
    /// and the event to wait for. What this adds is a service that is not
    /// running at all, which is a different error and a different fix.
    pub fn call(&self, method: &str, params: Value) -> Result<Value> {
        self.child
            .as_ref()
            .with_context(|| {
                format!(
                    "`{}` is not running, so `{method}` has nowhere to go. Start it with                      `plugin.enable {}`.",
                    self.instance, self.spec.plugin.plugin.name
                )
            })?
            .call(method, params)
    }

    /// The same, with a deadline of its own. What a transition uses: a plugin
    /// that cannot describe a wipe quickly is not one a take waits five
    /// seconds for.
    pub fn call_within(&self, method: &str, params: Value, within: Duration) -> Result<Value> {
        self.caller()
            .with_context(|| format!("`{}` is not running, so `{method}` has nowhere to go", self.instance))?
            .call_within(method, params, within)
    }

    /// A handle that makes one call without this service.
    ///
    /// For a caller that holds a lock over the table this service lives in.
    /// Take the handle, drop the lock, then call: see `Sidecar::caller`.
    pub fn caller(&self) -> Option<super::Caller> {
        self.child.as_ref().map(super::Sidecar::caller)
    }

    /// What the plugin is called, for an error and for `plugin.reload`.
    pub fn plugin(&self) -> &str {
        &self.spec.plugin.plugin.name
    }

    /// Whether this instance declared `[[tools]]` the core can route to it.
    pub fn tools(&self) -> Vec<String> {
        self.spec.plugin.tools.iter().map(|t| t.name.clone()).collect()
    }

    pub fn configure(&mut self, params: &Params) -> Result<Configure> {
        let child = self.child.as_ref().context("the plugin is not running")?;
        let answer = child.call("configure", json!({ "params": params_json(params) }))?;
        if answer.get("applied").and_then(Value::as_bool).unwrap_or(false) {
            return Ok(Configure::Applied);
        }
        Ok(Configure::RestartRequired(
            answer
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("the plugin did not say why")
                .to_string(),
        ))
    }

    pub fn health(&self) -> Health {
        let Some(child) = self.child.as_ref() else {
            return Health::of(PluginState::Stopped);
        };
        if !self.spec.manifest.capabilities.has(crate::plugin::Capability::Health) {
            return Health::of(state_of(child.state()));
        }
        match child.health() {
            Ok(h) => Health {
                state: match h.state {
                    HealthState::Ok => state_of(child.state()),
                    HealthState::Degraded => PluginState::Degraded,
                    HealthState::Failing => PluginState::Failed,
                },
                detail: h.detail,
            },
            Err(e) => {
                debug!(service = %self.instance, ?e, "the plugin did not answer `health`");
                Health {
                    state: PluginState::Degraded,
                    detail: Some("it did not answer `health` in time".into()),
                }
            }
        }
    }

    /// Call one of the plugin's `[[tools]]`, in MCP's shape.
    pub fn tool_call(&self, name: &str, arguments: Value) -> Result<Value> {
        let child = self.child.as_ref().context(
            "the plugin is not running, so its tools cannot be called. Enable it with \
             plugin.enable, or read plugin.list to see why it stopped.",
        )?;
        child.call("tool.call", json!({ "name": name, "arguments": arguments }))
    }

    /// Answer a request the plugin made of the core, once the caller has
    /// carried it out.
    /// Ask what is out there. Candidates come back with `params` ready for
    /// `source.add`, and each names the provide id that would open it.
    ///
    /// On the service rather than on the device, because the supervisor keeps
    /// one table of instances and asks the ones whose kind is `device`. A
    /// plugin that is not a device answers `-32601` and the message says so,
    /// which is a better error than one this side could invent.
    pub fn discover(&self, timeout: Duration) -> Result<Vec<Candidate>> {
        let child = self.child.as_ref().context(
            "the discovery plugin is not running. Enable it with plugin.enable, or read \
             plugin.list to see why it stopped.",
        )?;
        let within = timeout.min(DISCOVER_TIMEOUT);
        let value = child.call_within(
            "discover",
            json!({ "timeout_ms": within.as_millis() as u64 }),
            within + Duration::from_millis(500),
        )?;
        let found: Discovered = serde_json::from_value(value)
            .context("the plugin's `discover` answer was not {candidates: [...]}")?;
        Ok(found.candidates)
    }

    pub fn answer(&self, id: &Value, result: Value) -> Result<()> {
        self.child
            .as_ref()
            .context("the plugin is not running")?
            .answer_request(id, Ok(result))
    }
}

impl Drop for SidecarService {
    fn drop(&mut self) {
        self.stop("the plugin was removed");
    }
}

/// A `device` provide: the same process, asked a different question.
pub struct SidecarDevice(SidecarService);

impl SidecarDevice {
    pub fn new(spec: SidecarSpec) -> Self {
        Self(SidecarService::new(spec))
    }

    pub fn start(&mut self, canvas: &CanvasCaps, params: &Params) -> Result<()> {
        self.0.start(canvas, params)
    }

    pub fn stop(&mut self, reason: &str) {
        self.0.stop(reason)
    }

    pub fn instance(&self) -> &str {
        self.0.instance()
    }

    pub fn instance_state(&self) -> InstanceState {
        self.0.instance_state()
    }

    pub fn pid(&self) -> Option<u32> {
        self.0.pid()
    }

    pub fn health(&self) -> Health {
        self.0.health()
    }

    /// Ask what is out there. Candidates come back with `params` ready for
    /// `source.add`, and each names the provide id that would open it.
    pub fn discover(&self, timeout: Duration) -> Result<Vec<Candidate>> {
        self.0.discover(timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_discover_call_never_outlives_the_protocols_ceiling() {
        assert!(
            DISCOVER_TIMEOUT < Duration::from_secs(godwinmix_protocol::MAX_CALL_SECS),
            "a discovery that takes longer than a call is allowed to must answer with a task"
        );
    }
}
