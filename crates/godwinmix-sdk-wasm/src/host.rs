//! What a plugin may call back, as plain functions.
//!
//! Five of them. Anything else a plugin wants goes through `core_call`, whose
//! allow list is short and whose refusal names the whole of it, so the
//! boundary is learned from an error rather than from a hang.

use crate::bindings::godwinmix::plugin::host as raw;
use crate::bindings::godwinmix::plugin::types::LogLevel;
use serde_json::Value;

/// How loud a log line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<Level> for LogLevel {
    fn from(level: Level) -> LogLevel {
        match level {
            Level::Trace => LogLevel::Trace,
            Level::Debug => LogLevel::Debug,
            Level::Info => LogLevel::Info,
            Level::Warn => LogLevel::Warn,
            Level::Error => LogLevel::Error,
        }
    }
}

/// A line in the core's log, tagged with this instance.
pub fn log(level: Level, message: impl AsRef<str>) {
    raw::log(level.into(), message.as_ref());
}

/// Raise `event/<name>` on the core's event stream.
pub fn event(name: &str, params: &Value) {
    raw::event(name, &params.to_string());
}

/// Call the core. Read methods, and `program.take` when it was granted.
///
/// The error is the protocol's own: a code from 03 section 6 and a message
/// that names the next step.
pub fn core_call(method: &str, params: &Value) -> Result<Value, String> {
    match raw::core_call(method, &params.to_string()) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| format!("`{method}` answered something that was not JSON: {e}")),
        Err(e) => Err(format!("{} (code {})", e.message, e.code)),
    }
}

/// Milliseconds since the unix epoch, on the core's clock.
pub fn now_ms() -> u64 {
    raw::now_ms()
}

/// The pipeline's running time in nanoseconds, 0 before it runs.
///
/// This is the clock a take is scheduled on. A plugin measuring the gap
/// between two takes should prefer it to [`now_ms`], and fall back to
/// [`now_ms`] when it reads 0, which is what a core with no pipeline yet
/// reports.
pub fn running_time_ns() -> u64 {
    raw::running_time_ns()
}
