//! Channels on the wire: the records `channel.*` answers with and the
//! requests it takes. A channel takes its streams over RTMP, RTMPS, SRT and
//! WHIP with one set of keys; `crate::channel_ingest` has those shapes.
//!
//! The shapes are the ones in `dev/plans/channels-contract.md`, which was
//! written before either half was built so the server and the page could be
//! written against the same thing at once.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use crate::channel_ingest::{
    CertificateGenerateRequest, CertificateInfo, CertificateSetRequest, ChannelProtocol, Listener,
    PublishAddress, Rtmps,
};
use crate::channel_ingest::rtmp_only;
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

/// A named place encoders publish to, over every protocol it has switched on.
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
    /// The protocols it takes publishers over, besides RTMPS.
    #[serde(default = "rtmp_only")]
    pub protocols: Vec<ChannelProtocol>,
    /// RTMPS, on a port of its own, when a person has turned it on.
    #[serde(default)]
    pub rtmps: Rtmps,
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
    /// True when a person typed the secret (`secret` on `channel.add` or
    /// `channel.key.add`), usually to keep a password their encoders already
    /// send; false when the mixer made it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub imported: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
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
    /// The same for every protocol the channel has on, RTMP first.
    #[serde(default)]
    pub addresses: Vec<PublishAddress>,
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
    /// How it arrived: `rtmp`, `rtmps`, `srt` or `whip`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    pub video: Option<StreamVideo>,
    pub audio: Option<StreamAudio>,
    /// The mixer source it feeds, when it feeds one.
    pub source: Option<String>,
    /// Whole GOPs readers of it have lost by falling behind, this session.
    #[serde(default)]
    pub dropped_gops: u64,
    /// Where a mixer on this machine reads it: the listener's own port on
    /// loopback. Any show adds it as a source with `source.add {type:
    /// "ingest/rtmp", relay, stream: "<app>/<name>"}`, and every show that
    /// does reads the one stream the station received.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay: Option<String>,
    /// Why the mixer would not make this stream a source, while it will not:
    /// the stream is in, and nothing in a scene can show it. Gone once it does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_error: Option<String>,
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
    /// Every listener a channel needs, open or not, and why: the ports this
    /// mixer has open for ingest, and the channels each is open for.
    #[serde(default)]
    pub listeners: Vec<Listener>,
    /// The addresses an encoder can reach this machine at, first one first.
    #[serde(default)]
    pub hosts: Vec<String>,
    /// The certificate RTMPS answers with, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate: Option<CertificateInfo>,
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
    /// The application name encoders put after the port, as they already
    /// type it: `Church`, or `Youth Hall`. Defaults to a slug of the name.
    /// Matched without regard to case, so two channels cannot differ only in
    /// case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_source: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_mode: Option<KeyMode>,
    /// Defaults to RTMP alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocols: Option<Vec<ChannelProtocol>>,
    /// The first key's secret, when encoders already send one (the password
    /// after `?psk=`): 6 to 128 letters, digits, `-`, `_`, `.`, `~` or single
    /// spaces between them. Left out, the mixer makes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
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
    /// Which protocols it takes, as a whole list: `["rtmp", "srt"]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocols: Option<Vec<ChannelProtocol>>,
    /// RTMPS on or off, and its port.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtmps: Option<Rtmps>,
}

/// `channel.key.add`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChannelKeyAddRequest {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The secret to keep, when encoders already send one. The same rule as
    /// `secret` on `channel.add`. Left out, the mixer makes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
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

/// `channel.thumbnail`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChannelThumbnailRequest {
    /// The channel.
    pub id: String,
    /// Which of its streams. The first live one when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>,
    /// Pixels across, 16 to 640, made even. 320 when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
}
