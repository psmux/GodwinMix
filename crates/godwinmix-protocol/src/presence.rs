//! Who else is here: client ids, `presence.list` and `event/presence.changed`.
//!
//! A token says what a caller may do. It does not say which device is calling,
//! and one token is routinely shared by a phone, a tablet and a laptop at the
//! same show. So every caller also has a client id: the token id, a dot, and a
//! name for the connection (`default.s3`, `default.phone-cam`). The core gives
//! every `/rpc` connection one by itself; a caller that wants a stable one
//! (so a phone that reconnects keeps its undo stack) chooses it with
//! `client_id` on the `/rpc` URL or in the call envelope. The client id is what
//! scene patches carry as `source_client`, what owns an undo stack and a draft,
//! and what presence lists.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The longest name a caller may give its connection.
pub const MAX_CLIENT_NAME: usize = 32;

/// Is this a name a caller may give its connection: a slug of lower case
/// letters, digits and dashes?
pub fn valid_client_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_CLIENT_NAME
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The client id for a token and a connection name: `default.phone-cam`.
pub fn client_id(token: &str, name: &str) -> String {
    format!("{token}.{name}")
}

/// One client connected to `/rpc`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PresenceClient {
    /// The client id, as scene patches carry it in `source_client`.
    pub client_id: String,
    /// The token it connected with.
    pub token: String,
    /// What to call it: the name the client gave itself with presence.set,
    /// else the token's label when the token has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// A guess at the device from its User-Agent: "iPhone Safari",
    /// "Windows Chrome", "gmx CLI". Empty when it sent none.
    pub device: String,
    /// The scene this client says it is editing, when it said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    /// When it connected, in milliseconds since the Unix epoch.
    pub since_ms: u64,
    /// True for the connection asking.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub you: bool,
}

/// `presence.list`, and the payload of `event/presence.changed`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PresenceList {
    /// Every connected client, oldest connection first.
    pub clients: Vec<PresenceClient>,
}

/// `presence.set`: what this connection tells everybody else about itself.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct PresenceSetRequest {
    /// The scene this client is editing, by id or name. Null or omitted says it
    /// is editing none.
    #[serde(default)]
    pub scene: Option<String>,
    /// A name for this device that a person chose ("Sam's phone"). Omitted
    /// keeps the one it has; an empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// `event/presence.changed`.
pub fn events() -> Vec<crate::protocol::EventDef> {
    vec![crate::protocol::EventDef {
        name: "presence.changed",
        since: "1",
        summary: "Somebody connected to /rpc, left, or said which scene they are editing. \
                  Carries the whole list, as presence.list answers it. Sent only to a \
                  client that subscribed to it, and nothing is worked out while nobody has.",
        ext: None,
        legacy: None,
        payload: crate::method::schema_of::<PresenceList>,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_name_is_a_short_slug() {
        assert!(valid_client_name("phone-cam-2"));
        assert!(!valid_client_name(""));
        assert!(!valid_client_name("Phone"));
        assert!(!valid_client_name("a.b"));
        assert!(!valid_client_name(&"x".repeat(MAX_CLIENT_NAME + 1)));
        assert_eq!(client_id("default", "s3"), "default.s3");
    }
}
