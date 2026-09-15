//! Write a GodwinMix plugin as a WebAssembly component.
//!
//! The same shape as `godwinmix-sdk`, which is the crate for a plugin that is
//! a process: implement a trait, hand the type to a macro, and the framing is
//! somebody else's problem. The difference is what the framing is. A process
//! writes JSON lines on a pipe; a component exports functions. Both carry the
//! same methods and the same JSON, so a plugin moves between the two by
//! changing which crate it depends on.
//!
//! ```ignore
//! use godwinmix_sdk_wasm::{export_service, Hello, Ready, Service};
//!
//! struct MinHold { last: u64 }
//!
//! impl Service for MinHold {
//!     fn initialize(hello: &Hello) -> Result<(Self, Ready), String> {
//!         Ok((MinHold { last: 0 }, Ready::new("min-hold", "0.2.0").hook("take.before")))
//!     }
//!
//!     fn hook(&mut self, name: &str, payload: &serde_json::Value)
//!         -> Result<serde_json::Value, String>
//!     {
//!         Ok(serde_json::json!({ "allow": true }))
//!     }
//! }
//!
//! export_service!(MinHold);
//! ```
//!
//! Build it with `cargo build --release --target wasm32-wasip2`. That target
//! emits a component, so there is no `cargo component` and no `wasm-tools` in
//! the loop; the `.wasm` it writes is the file `[run] wasm` names.
//!
//! # What crosses the boundary
//!
//! JSON, as a string, exactly as a process plugin would have written it on its
//! line. Nothing is reshaped. The one thing that never crosses is media: there
//! is no frame and no pad here, and a `source` at this placement is refused.

pub mod bindings;
#[macro_use]
mod glue;
pub mod wire;
pub mod service;
pub mod transition;

mod host;

pub use host::{core_call, event, log, now_ms, running_time_ns, Level};
pub use service::Service;
pub use transition::{Answer, Transition};

use serde_json::Value;

/// The handshake, as a plugin reads it.
#[derive(Debug, Clone)]
pub struct Hello {
    pub core: String,
    pub core_version: String,
    pub api_level: u32,
    pub api_compatible: u32,
    pub width: u32,
    pub height: u32,
    pub fps_n: u32,
    pub fps_d: u32,
    pub instance: String,
    pub provide: String,
    /// The validated `params` object. `Value::Null` when the operator wrote
    /// none, so a plugin reads it with `get(..)` and defaults rather than
    /// matching on a shape.
    pub params: Value,
    pub granted: Granted,
}

impl Hello {
    /// The canvas frame rate as a number, which is what most plugins want.
    pub fn fps(&self) -> f64 {
        if self.fps_d == 0 {
            return 0.0;
        }
        f64::from(self.fps_n) / f64::from(self.fps_d)
    }

    /// One setting off `params`, or the default.
    pub fn param_u64(&self, key: &str, default: u64) -> u64 {
        self.params.get(key).and_then(Value::as_u64).unwrap_or(default)
    }

    pub fn param_str(&self, key: &str, default: &str) -> String {
        self.params.get(key).and_then(Value::as_str).unwrap_or(default).to_string()
    }
}

/// What the host granted. A plugin reads it and adapts; it never decides it.
#[derive(Debug, Clone, Copy, Default)]
pub struct Granted {
    pub core_call: bool,
    pub take: bool,
    pub events: bool,
    pub filesystem: bool,
    pub network: bool,
    pub fuel_per_call: u64,
    pub deadline_ms: u32,
    pub max_memory_mb: u32,
}

/// What a plugin answers the handshake with.
#[derive(Debug, Clone)]
pub struct Ready {
    pub plugin: String,
    pub version: String,
    pub api: u32,
    pub tools: Vec<String>,
    pub hooks: Vec<String>,
    pub latency_ms: Option<u32>,
}

impl Ready {
    /// The minimum: a name and a version. `api` is 1, which is the level this
    /// SDK was written against.
    pub fn new(plugin: &str, version: &str) -> Ready {
        Ready {
            plugin: plugin.to_string(),
            version: version.to_string(),
            api: 1,
            tools: Vec::new(),
            hooks: Vec::new(),
            latency_ms: None,
        }
    }

    /// One of the plugin's `[[tools]]`, by its unprefixed name.
    pub fn tool(mut self, name: &str) -> Ready {
        self.tools.push(name.to_string());
        self
    }

    /// One of 03 section 8's hooks. The host only calls `hook` for names that
    /// are in this list and in the manifest's `[hooks]` table, so a plugin
    /// that forgets one here is never asked.
    pub fn hook(mut self, name: &str) -> Ready {
        self.hooks.push(name.to_string());
        self
    }

    pub fn latency(mut self, ms: u32) -> Ready {
        self.latency_ms = Some(ms);
        self
    }
}

/// What a plugin says when asked how it is.
#[derive(Debug, Clone)]
pub struct Health {
    pub state: HealthState,
    pub detail: Option<String>,
    pub latency_ms: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthState {
    Ok,
    Degraded,
    Failing,
}

impl Default for Health {
    fn default() -> Self {
        Health { state: HealthState::Ok, detail: None, latency_ms: None }
    }
}

impl Health {
    pub fn degraded(detail: &str) -> Health {
        Health {
            state: HealthState::Degraded,
            detail: Some(detail.to_string()),
            latency_ms: None,
        }
    }
}

/// The answer to `configure`.
#[derive(Debug, Clone)]
pub struct Configured {
    pub applied: bool,
    pub restart_required: bool,
    pub reason: Option<String>,
}

impl Configured {
    pub fn applied() -> Configured {
        Configured { applied: true, restart_required: false, reason: None }
    }

    /// The new params cannot be taken without a restart, and why.
    pub fn restart(reason: &str) -> Configured {
        Configured {
            applied: false,
            restart_required: true,
            reason: Some(reason.to_string()),
        }
    }
}

/// A `take.before` answer that lets the take through.
pub fn allow() -> Value {
    serde_json::json!({ "allow": true })
}

/// A `take.before` answer that refuses it, with the reason the operator sees.
///
/// The reason is the whole of the error an operator or an agent reads, so it
/// should name what is wrong and what to do, in that order.
pub fn refuse(reason: impl Into<String>) -> Value {
    serde_json::json!({ "allow": false, "reason": reason.into() })
}

/// The MCP shaped result a `tool.call` answers with.
pub fn tool_text(text: impl Into<String>) -> Value {
    serde_json::json!({ "content": [{ "type": "text", "text": text.into() }] })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_carries_the_reason_and_an_allow_does_not_need_one() {
        assert_eq!(allow()["allow"], serde_json::json!(true));
        let no = refuse("wait 3.2 s");
        assert_eq!(no["allow"], serde_json::json!(false));
        assert_eq!(no["reason"], serde_json::json!("wait 3.2 s"));
    }

    #[test]
    fn ready_is_built_up_rather_than_filled_in() {
        let r = Ready::new("min-hold", "0.2.0").hook("take.before").tool("hold_state");
        assert_eq!(r.api, 1);
        assert_eq!(r.hooks, ["take.before"]);
        assert_eq!(r.tools, ["hold_state"]);
    }
}
