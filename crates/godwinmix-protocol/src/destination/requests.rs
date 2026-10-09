//! The three destination methods' params.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use crate::rendition::RenditionChoice;

/// `channel.destination.add`. Send a channel's stream on to a platform.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct AddDestinationRequest {
    /// The channel.
    pub id: String,
    /// youtube, facebook, twitch, custom or srt; or `file` to record the
    /// stream on this machine, or `hls` to serve it as a watch link.
    pub platform: String,
    /// What the list calls it. The platform's name when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The ingest address. Left out, the platform's own; custom and srt need
    /// one. For `file`, a folder on the mixer (the recordings folder when
    /// left out); for `hls`, `hls://` with params such as `?segment_ms=2000`.
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
    /// Convert the stream before sending it: `{"preset": "youtube-720p30"}`
    /// or a rendition request written out. Left out, or one the stream
    /// already matches, the stream is sent as it arrives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rendition: Option<RenditionChoice>,
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
    /// A new rendition. Left out keeps the one it has; `null` or
    /// `{"preset": "copy"}` goes back to sending the stream as it arrives.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<RenditionChoice>")]
    pub rendition: Option<Option<RenditionChoice>>,
}

/// A field that is there, even as `null`, is `Some`; one left out is `None`.
fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<RenditionChoice>>, D::Error> {
    Option::<RenditionChoice>::deserialize(d).map(Some)
}

/// `channel.destination.remove`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RemoveDestinationRequest {
    /// The channel.
    pub id: String,
    /// The destination's id within the channel.
    pub destination: String,
}
