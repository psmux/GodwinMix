//! Between the plain types an author writes against and the generated ones.
//!
//! Small and dull on purpose. Every function here is a field for field copy,
//! because the two shapes are the same shape and the only reason both exist is
//! that one of them is generated and cannot carry a helper method.

use crate::bindings::exports::godwinmix::plugin::transition as tr;
use crate::bindings::godwinmix::plugin::types as wit;
use crate::{Answer, Configured, Granted, Health, HealthState, Hello, Ready};
use serde_json::Value;

pub fn hello(h: wit::Hello) -> Hello {
    Hello {
        core: h.core,
        core_version: h.core_version,
        api_level: h.api_level,
        api_compatible: h.api_compatible,
        width: h.canvas.width,
        height: h.canvas.height,
        fps_n: h.canvas.fps_n,
        fps_d: h.canvas.fps_d,
        instance: h.instance,
        provide: h.provide,
        params: json(&h.params_json),
        granted: Granted {
            core_call: h.granted.core_call,
            take: h.granted.take,
            events: h.granted.events,
            filesystem: h.granted.filesystem,
            network: h.granted.network,
            fuel_per_call: h.granted.fuel_per_call,
            deadline_ms: h.granted.deadline_ms,
            max_memory_mb: h.granted.max_memory_mb,
        },
    }
}

pub fn ready(r: &Ready) -> wit::Ready {
    wit::Ready {
        plugin: r.plugin.clone(),
        version: r.version.clone(),
        api: r.api,
        tools: r.tools.clone(),
        hooks: r.hooks.clone(),
        latency_ms: r.latency_ms,
    }
}

pub fn health(h: &Health) -> wit::HealthReport {
    wit::HealthReport {
        state: match h.state {
            HealthState::Ok => wit::HealthState::Ok,
            HealthState::Degraded => wit::HealthState::Degraded,
            HealthState::Failing => wit::HealthState::Failing,
        },
        detail: h.detail.clone(),
        latency_ms: h.latency_ms,
    }
}

pub fn configured(c: &Configured) -> wit::Configured {
    wit::Configured {
        applied: c.applied,
        restart_required: c.restart_required,
        reason: c.reason.clone(),
    }
}

pub fn answer(a: Answer) -> tr::Answer {
    match a {
        Answer::Pads(v) => tr::Answer::Pads(v.to_string()),
        Answer::Curve(v) => tr::Answer::Curve(v.to_string()),
    }
}

pub fn error(code: i32, message: String) -> wit::Error {
    wit::Error { code, message, data_json: None }
}

/// Params that will not parse are `null` rather than a failure.
///
/// The host validated them against the schema before sending, so a string that
/// is not JSON here means something upstream is wrong and a plugin refusing to
/// start over it would only hide that. Reading `null` and falling back to
/// defaults is the behaviour that keeps a show on air.
pub fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or(Value::Null)
}
