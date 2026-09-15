//! What a component may call back, and the door it calls the core through.
//!
//! Four of the five are answered here and cost nothing: a log line, an event,
//! and two clocks. The fifth, `core-call`, is a narrow door onto the same
//! JSON-RPC methods every other client uses, and the list is short on purpose.
//! A plugin that wants the rest of the protocol runs as a process and gets
//! `GMX_RPC` and a token, which is the placement that was designed for it.

use crate::bindings::types::{Error, LogLevel};
use godwinmix_core::plugin::wasm::Grant;
use parking_lot::Mutex;
use serde_json::Value;
use std::sync::{Arc, OnceLock};

/// The read methods a component may call, and `program.take`.
///
/// Read methods are always in. `program.take` is in only when the grant says
/// so, which follows the hooks the plugin asked for: a policy service that
/// watches takes may make one, a plugin that never sees a take may not.
pub const READ_METHODS: &[&str] = &[
    "core.info",
    "core.api",
    "agent.state",
    "codec.list",
    "program.get",
    "program.history",
    "scene.list",
    "scene.get",
    "source.list",
    "source.get",
    "output.list",
    "output.get",
    "filter.list",
    "plugin.list",
    "plugin.describe",
    "tool.list",
];

pub const TAKE_METHOD: &str = "program.take";

/// How the core answers a `core-call`. Registered by the binary, which is the
/// only crate that has a control plane to dispatch into.
pub type CoreCall = Arc<dyn Fn(&str, Value) -> Result<Value, (i32, String)> + Send + Sync>;

/// Where a component's events go. Registered by the binary too, for the same
/// reason: the event bus belongs to the running core, not to this crate.
pub type Emit = Arc<dyn Fn(&str, &str, Value) + Send + Sync>;

/// Where the pipeline's running time comes from.
pub type RunningTime = Arc<dyn Fn() -> u64 + Send + Sync>;

static CORE_CALL: OnceLock<CoreCall> = OnceLock::new();
static EMIT: OnceLock<Emit> = OnceLock::new();
static RUNNING_TIME: Mutex<Option<RunningTime>> = Mutex::new(None);

/// Tell this crate how to reach the core. Called once, at startup.
pub fn wire(core_call: CoreCall, emit: Emit) {
    let _ = CORE_CALL.set(core_call);
    let _ = EMIT.set(emit);
}

/// Tell it where the pipeline clock is. Separate from `wire` because the mixer
/// is built after the control plane and a core may have no mixer at all.
pub fn wire_clock(clock: RunningTime) {
    *RUNNING_TIME.lock() = Some(clock);
}

/// Everything one instance's host functions need.
pub struct Callbacks {
    pub plugin: String,
    pub instance: String,
    pub grant: Grant,
}

impl Callbacks {
    pub fn log(&self, level: LogLevel, message: String) {
        let instance = self.instance.as_str();
        match level {
            LogLevel::Trace => tracing::trace!(%instance, "{message}"),
            LogLevel::Debug => tracing::debug!(%instance, "{message}"),
            LogLevel::Info => tracing::info!(%instance, "{message}"),
            LogLevel::Warn => tracing::warn!(%instance, "{message}"),
            LogLevel::Error => tracing::error!(%instance, "{message}"),
        }
    }

    pub fn event(&self, name: String, params_json: String) {
        if !self.grant.events {
            tracing::debug!(instance = %self.instance, %name, "an event was dropped: not granted");
            return;
        }
        let params = match serde_json::from_str::<Value>(&params_json) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    instance = %self.instance, %name, %e,
                    "a component raised an event whose params were not JSON; it was dropped"
                );
                return;
            }
        };
        match EMIT.get() {
            Some(emit) => emit(&self.plugin, &name, params),
            None => tracing::debug!(
                instance = %self.instance, %name,
                "a component raised an event and this core has no event bus to put it on"
            ),
        }
    }

    pub fn core_call(&self, method: String, params_json: String) -> Result<String, Error> {
        if !self.grant.core_call {
            return Err(refused(format!(
                "`{}` was not granted `core-call`. Nothing outside the component is reachable \
                 from it.",
                self.instance
            )));
        }
        if !allowed(&method, self.grant.take) {
            return Err(refused(format!(
                "`{method}` is not one a component may call. A component may call: {}{}. \
                 Anything else is what the `sidecar` placement is for: it gets GMX_RPC and a \
                 token and the whole protocol.",
                READ_METHODS.join(", "),
                if self.grant.take { format!(", {TAKE_METHOD}") } else { String::new() }
            )));
        }
        let params = serde_json::from_str::<Value>(&params_json).map_err(|e| Error {
            code: -32602,
            message: format!("the params of `{method}` were not JSON: {e}"),
            data_json: None,
        })?;
        let Some(call) = CORE_CALL.get() else {
            return Err(Error {
                code: -32001,
                message: format!(
                    "this core has no control plane for `{method}` to reach. That happens in a \
                     test and in an embedded core; a running mixer always has one."
                ),
                data_json: None,
            });
        };
        match call(&method, params) {
            Ok(value) => Ok(value.to_string()),
            Err((code, message)) => Err(Error { code, message, data_json: None }),
        }
    }

    pub fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    pub fn running_time_ns(&self) -> u64 {
        RUNNING_TIME.lock().as_ref().map(|clock| clock()).unwrap_or(0)
    }
}

fn allowed(method: &str, take: bool) -> bool {
    READ_METHODS.contains(&method) || (take && method == TAKE_METHOD)
}

fn refused(message: String) -> Error {
    Error { code: -32002, message, data_json: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn callbacks(grant: Grant) -> Callbacks {
        Callbacks { plugin: "demo".into(), instance: "demo-hold".into(), grant }
    }

    #[test]
    fn a_read_method_is_allowed_and_a_write_is_not() {
        assert!(allowed("program.get", false));
        assert!(!allowed("source.remove", true));
        assert!(!allowed(TAKE_METHOD, false));
        assert!(allowed(TAKE_METHOD, true));
    }

    #[test]
    fn a_refusal_names_the_whole_allow_list_and_the_placement_that_has_more() {
        let c = callbacks(Grant::default());
        let e = c.core_call("source.remove".into(), "{}".into()).expect_err("not allowed");
        assert_eq!(e.code, -32002);
        assert!(e.message.contains("program.get"), "{}", e.message);
        assert!(e.message.contains("sidecar"), "{}", e.message);
    }

    #[test]
    fn a_component_with_no_core_call_grant_is_told_so_rather_than_left_waiting() {
        let c = callbacks(Grant { core_call: false, ..Grant::default() });
        let e = c.core_call("program.get".into(), "{}".into()).expect_err("not granted");
        assert_eq!(e.code, -32002);
        assert!(e.message.contains("core-call"), "{}", e.message);
    }

    #[test]
    fn a_clock_nobody_wired_reads_zero_rather_than_panicking() {
        let c = callbacks(Grant::default());
        assert_eq!(c.running_time_ns(), 0);
        assert!(c.now_ms() > 1_700_000_000_000, "the wall clock is the wall clock");
    }
}
