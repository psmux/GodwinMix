//! Why a destination is not connected, on its `OutputStatus` as `error`.
//!
//! Before this, a destination that never got through said "Reconnecting,
//! attempt 3" and nothing else. The sink knew exactly what was wrong (the
//! server refused us, the name does not exist, the platform turned the key
//! away) and the core threw that away on the bus. A person staring at a
//! YouTube page that says "No data" could not tell their key from their
//! firewall.
//!
//! `reason` is for a client that wants to decide something (which field to
//! outline, which help to link). `message` is for a person and says what to
//! do next. `detail` is GStreamer's own sentence, with anything that could be
//! a stream key cut out, for whoever is reading a support thread.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What went wrong the last time this destination tried.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OutputError {
    pub reason: OutputErrorReason,
    /// One or two sentences for a person: what happened and what to try.
    pub message: String,
    /// The sink's own words with the key cut out. Empty when it said nothing.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

/// The kinds of failure a client may want to tell apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum OutputErrorReason {
    /// The host answered and nothing was listening on that port.
    Refused,
    /// No route to the host, or the network is down.
    Unreachable,
    /// The host never answered.
    TimedOut,
    /// The server name does not exist.
    NotFound,
    /// The server answered and turned the stream away, which for a platform
    /// is a stream key it does not know.
    Rejected,
    /// The server hung up during the handshake or as the stream started.
    Closed,
    /// The server took the connection and never took the stream: no error,
    /// no data accepted, until the outage buffer filled. Some servers answer
    /// a key they do not know this way.
    Stalled,
    /// Anything else. `detail` says what.
    Other,
}

impl OutputErrorReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Refused => "refused",
            Self::Unreachable => "unreachable",
            Self::TimedOut => "timed-out",
            Self::NotFound => "not-found",
            Self::Rejected => "rejected",
            Self::Closed => "closed",
            Self::Stalled => "stalled",
            Self::Other => "other",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reason_is_kebab_case_on_the_wire() {
        let e = OutputError {
            reason: OutputErrorReason::TimedOut,
            message: "m".into(),
            detail: String::new(),
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["reason"], "timed-out");
        assert!(v.get("detail").is_none(), "an empty detail is left out");
        for r in [OutputErrorReason::Refused, OutputErrorReason::NotFound, OutputErrorReason::Other] {
            assert_eq!(serde_json::to_value(r).unwrap(), r.as_str());
        }
    }
}
