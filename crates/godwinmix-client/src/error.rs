//! The one error shape.
//!
//! Every refusal from the core arrives as `{code, message, data}` and the
//! message already names the current state and the next step, so a surface
//! shows it as it came rather than inventing wording of its own.

use std::fmt;

use serde_json::Value;

pub type Result<T> = std::result::Result<T, Error>;

/// Codes the core uses. Anything else falls through to the generic branch.
pub mod codes {
    pub const PARSE: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const NO_METHOD: i32 = -32601;
    pub const BAD_PARAMS: i32 = -32602;
    pub const INTERNAL: i32 = -32603;
    pub const WRONG_STATE: i32 = -32001;
    pub const NO_SCOPE: i32 = -32002;
    pub const SAFETY: i32 = -32003;
    pub const NOT_FOUND: i32 = -32004;
    pub const NO_PLACEMENT: i32 = -32005;
    pub const PLUGIN_DIED: i32 = -32010;
    pub const LINE_TOO_LONG: i32 = -32011;
    pub const RESTART_REQUIRED: i32 = -32012;
    pub const CONFIRM_REQUIRED: i32 = -32020;
}

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// The core refused the call and said why.
    Rpc { code: i32, message: String, data: Value },
    /// The socket, the handshake or the address.
    Transport(String),
    /// A result that did not fit the type this api_level expects.
    Decode(String),
    /// The connection went away while a call was outstanding.
    Closed,
}

impl Error {
    pub fn code(&self) -> Option<i32> {
        match self {
            Error::Rpc { code, .. } => Some(*code),
            _ => None,
        }
    }

    /// True when the same call could plausibly work if tried again.
    pub fn retryable(&self) -> bool {
        match self {
            Error::Rpc { code, data, .. } => {
                if let Some(flag) = data.get("retryable").and_then(Value::as_bool) {
                    return flag;
                }
                matches!(*code, codes::WRONG_STATE | codes::SAFETY | codes::PLUGIN_DIED)
            }
            Error::Closed | Error::Transport(_) => true,
            Error::Decode(_) => false,
        }
    }

    /// Milliseconds to wait before a retry, when the core named one.
    pub fn retry_after_ms(&self) -> Option<u64> {
        match self {
            Error::Rpc { data, .. } => data.get("retry_after_ms").and_then(Value::as_u64),
            _ => None,
        }
    }

    /// The sentence after the last full stop, which by convention is the next
    /// step the operator should take.
    pub fn next_step(&self) -> Option<&str> {
        let message = match self {
            Error::Rpc { message, .. } => message.as_str(),
            _ => return None,
        };
        let mut parts: Vec<&str> = message.split(". ").filter(|s| !s.trim().is_empty()).collect();
        if parts.len() < 2 {
            return None;
        }
        parts.pop()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Rpc { code, message, .. } => write!(f, "{message} (code {code})"),
            Error::Transport(why) => write!(f, "the connection to the mixer failed: {why}"),
            Error::Decode(why) => write!(f, "the mixer answered something this build cannot read: {why}"),
            Error::Closed => write!(f, "the connection to the mixer closed before the call answered"),
        }
    }
}

impl std::error::Error for Error {}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Decode(e.to_string())
    }
}
