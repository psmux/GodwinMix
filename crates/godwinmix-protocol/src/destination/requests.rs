//! The three destination methods' params.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `channel.destination.add`. Send a channel's stream on to a platform.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct AddDestinationRequest {
    /// The channel.
    pub id: String,
    /// youtube, facebook, twitch, custom or srt.
    pub platform: String,
    /// What the list calls it. The platform's name when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The ingest address. Left out, the platform's own; custom and srt need
    /// one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    /// The stream key. Write only: no method reads it back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Which of the channel's streams to send. `*`, the default, is the first
    /// one live.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>,
    /// On by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// `channel.destination.set`. Change one destination, naming only what moves.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct SetDestinationRequest {
    /// The channel.
    pub id: String,
    /// The destination's id within the channel.
    pub destination: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    /// A new stream key. Left out keeps the one it has; an empty string
    /// clears it, where the platform allows none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// `channel.destination.remove`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RemoveDestinationRequest {
    /// The channel.
    pub id: String,
    /// The destination's id within the channel.
    pub destination: String,
}
