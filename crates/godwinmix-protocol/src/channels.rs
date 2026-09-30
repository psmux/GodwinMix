//! RTMP channels on the wire: the records `channel.*` answers with and the
//! requests it takes.
//!
//! The shapes are the ones in `dev/plans/channels-contract.md`, which was
//! written before either half was built so the server and the page could be
//! written against the same thing at once.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use crate::destination::Destination;

/// How a publisher gives its key.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum KeyMode {
    /// On the stream name: `main?psk=<key>`, or `key`, `token`, `Token`.
    #[default]
    Query,
    /// The stream name is the key, for an encoder with one box to type in.
    Stream,
}

/// A named place encoders publish to, on the mixer's own RTMP port.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Channel {
    /// A slug, and never changes once the channel exists.
    pub id: String,
    /// What a person calls it.
    pub name: String,
    /// The RTMP application name: the path segment after the port.
    pub app: String,
    /// Off turns every publisher away with a sentence saying so.
    pub enabled: bool,
    /// A stream that goes live becomes a mixer source by itself.
    pub auto_source: bool,
    pub key_mode: KeyMode,
    /// The keys as hints, never the key itself: a read token sees only these.
    pub keys: Vec<ChannelKey>,
    pub publish: ChannelPublish,
    /// Live streams, and streams that left while a scene still holds their
    /// source.
    pub streams: Vec<ChannelStream>,
    /// Where the channel's streams are sent on to, with what each is doing.
    /// Changed by `channel.destination.*`.
    #[serde(default)]
    pub destinations: Vec<Destination>,
}

/// One key, as a list shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelKey {
    pub id: String,
    pub label: String,
    /// When it was made, RFC 3339 in UTC.
    pub created: String,
    /// The last four characters, so a person can tell two keys apart.
    pub hint: String,
}

/// A key as it is made, with its secret. Afterwards only an admin gets the
/// secret again, one key at a time, from `channel.key.reveal`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NewKey {
    pub id: String,
    pub label: String,
    /// The key. A list never carries it; `channel.key.reveal` reads it back.
    pub secret: String,
}

/// Where an encoder is pointed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelPublish {
    /// `rtmp://<first address>:<port>/<app>`.
    pub server: String,
    /// `<server>/main?psk=<key>`, with `<key>` left for the person to fill.
    pub example: String,
}

/// One stream on a channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelStream {
    pub name: String,
    /// `live`, or `idle` for one that left while a scene holds its source.
    pub state: String,
    /// When it last went live, in milliseconds since 1970.
    pub since_ms: u64,
    /// The publisher's address.
    pub from: String,
    /// The id of the key that let it in.
    pub key: Option<String>,
    pub video: Option<StreamVideo>,
    pub audio: Option<StreamAudio>,
    /// The mixer source it feeds, when it feeds one.
    pub source: Option<String>,
    /// Whole GOPs readers of it have lost by falling behind, this session.
    #[serde(default)]
    pub dropped_gops: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StreamVideo {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub kbps: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StreamAudio {
    pub codec: String,
    pub channels: u32,
    pub sample_rate: u32,
    pub kbps: u32,
}

/// `channel.list`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChannelList {
    pub channels: Vec<Channel>,
    pub rtmp: RtmpInfo,
}

/// The RTMP port every channel shares.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RtmpInfo {
    pub port: u16,
    /// `rtmp://<address>:<port>` for each address this machine has.
    pub urls: Vec<String>,
    /// Whether the listener is running. False until the ingest plugin is
    /// installed and enabled.
    pub listening: bool,
    /// Why not, and what to do, when it is not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// `channel.add`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChannelAddRequest {
    pub name: String,
    /// Defaults to a slug of the name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_source: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_mode: Option<KeyMode>,
}

/// What `channel.add` answers: the channel and its first key.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChannelAdded {
    pub channel: Channel,
    pub key: NewKey,
}

/// `channel.set`: only what is named moves.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ChannelSetRequest {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_source: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_mode: Option<KeyMode>,
}

/// `channel.key.add`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChannelKeyAddRequest {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// What `channel.key.add` answers.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct KeyAdded {
    pub key: NewKey,
}

/// `channel.key.remove`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChannelKeyRemoveRequest {
    pub id: String,
    pub key: String,
}

/// `channel.key.reveal`: one key of one channel.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChannelKeyRevealRequest {
    pub id: String,
    pub key: String,
}

/// What `channel.key.reveal` answers: the key itself, and nothing a list
/// would carry.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct KeyRevealed {
    pub secret: String,
}

/// What `channel.remove` answers.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChannelRemoved {
    pub removed: String,
}
