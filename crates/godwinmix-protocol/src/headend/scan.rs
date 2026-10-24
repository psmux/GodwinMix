//! `headend.scan`: what a multiplex carries, read off the wire for a few
//! seconds by the same input a channel uses, so each program can be ticked
//! and made a channel.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `headend.scan`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScanRequest {
    /// The feed: `udp://@239.0.0.1:5000`, `rtp://@239.0.0.1:5000`, a
    /// unicast `udp://0.0.0.0:5000`, or any other MPEG-TS input a channel
    /// takes.
    pub uri: String,
    /// The card to join a multicast group on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface: Option<String>,
    /// The longest to listen, in milliseconds, 2000 to 20000. Left out: 8000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_ms: Option<u32>,
}

/// One elementary stream of a program, as its PMT names it and its frames
/// show it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScannedTrack {
    /// `video` or `audio`.
    pub kind: String,
    /// `h264`, `mpeg2`, `aac`, `ac3`.
    pub codec: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
    /// The picture is sent as fields (1080i, 480i).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub interlaced: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channels: Option<u32>,
    /// ISO 639 code from the PMT, `eng`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u16>,
}

/// One program of the multiplex.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScannedProgram {
    pub number: u16,
    /// From the SDT, when the feed sends one.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub provider: String,
    /// What the PMT lists, in its order: `h264`, `aac`, `teletext`.
    pub streams: Vec<String>,
    /// The video and sound tracks with what their frames said in the time
    /// listened. Empty when the program sent no frame in that time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tracks: Vec<ScannedTrack>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kbps: Option<u32>,
    /// Scrambled packets seen: a program whose key is missing here.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub scrambled: bool,
    /// The channel that already takes this program from this feed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
}

/// `headend.scan`'s answer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScanResult {
    pub uri: String,
    pub programs: Vec<ScannedProgram>,
    /// How long it listened.
    pub listened_ms: u64,
    /// Set when nothing was read: what the input said, and what to check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
