//! A headend channel as GodwinMix keeps it: set up on the headend screen or
//! by `headend.channel.add`, with no Flussonic file behind it. Each channel
//! runs as a direct show (compositing off), the same show a Flussonic
//! import makes: its input, one HLS output that every player and the
//! Flussonic compatible URLs read, and any further outputs.

use super::profile::TranscodeProfile;
use crate::shows::ShowRefused;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Where a channel's programme comes from.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChannelInput {
    /// The address, as a direct show's input takes it: `udp://@239.0.0.1:5000`
    /// (multicast), `udp://0.0.0.0:5000` (unicast to this machine),
    /// `rtp://@239.0.0.1:5000`, `srt://192.0.2.10:9000`, `srt://@:9000`,
    /// `rtmp://host/app/key`, `rtsp://192.0.2.20/stream1`,
    /// `https://example.com/live/index.m3u8` or `file:///clips/loop.ts`.
    pub uri: String,
    /// The MPEG-TS program number to take from a feed that carries several.
    /// Left out: the first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<u16>,
    /// The network card a multicast group is joined on, by name or by one
    /// of its IPv4 addresses. Left out: the one the default route uses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface: Option<String>,
    /// Further params the input takes, as `docs/reference/direct-inputs.md`
    /// lists them: `passphrase` and `latency_ms` for SRT, `transport` for
    /// RTSP, `stall_ms` and `return_ms` for the switch to the backup.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub params: Map<String, Value>,
    /// A second input, taken when this one stops and let go when it comes
    /// back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup: Option<ChannelBackup>,
}

/// A channel's backup input: an input with no backup of its own.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChannelBackup {
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface: Option<String>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub params: Map<String, Value>,
}

impl ChannelBackup {
    /// As an input, for what reads both the same way.
    pub fn as_input(&self) -> ChannelInput {
        ChannelInput { uri: self.uri.clone(), program: self.program, interface: self.interface.clone(), params: self.params.clone(), backup: None }
    }
}

/// The channel's HLS output, which every channel has.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChannelHls {
    /// `both` (MPEG-TS and CMAF, the default), `ts` or `cmaf`.
    #[serde(default = "both")]
    pub segments: String,
    /// `open` (the default): no viewer key, answered on loopback and private
    /// addresses only, for a gateway or cache that checks its own. `keyed`:
    /// a viewer key in every link.
    #[serde(default = "open")]
    pub access: String,
    /// The longest a segment may be, in milliseconds. Cut at a keyframe.
    #[serde(default = "segment_ms")]
    pub segment_ms: u32,
    /// Seconds of segments kept in the playlist. Left out: the packager's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<u32>,
}

fn both() -> String {
    "both".into()
}
fn open() -> String {
    "open".into()
}
fn segment_ms() -> u32 {
    3400
}

impl Default for ChannelHls {
    fn default() -> Self {
        ChannelHls { segments: both(), access: open(), segment_ms: segment_ms(), window: None }
    }
}

/// One output beside the HLS one: a UDP multicast or unicast out, an SRT
/// or RTMP push.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChannelOutput {
    /// A slug, unique in the channel. Left out: `push-1`, `push-2` in order.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// `udp://239.0.1.1:5000`, `srt://192.0.2.30:9000`, `rtmp://host/app/key`.
    pub uri: String,
    /// True: sent as the input comes, whatever the channel's profile. Left
    /// out: the same conversion as the HLS output.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub copy: bool,
}

/// One channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HeadendChannel {
    /// The channel's public name: letters, digits, `-`, `_` and `.`, up to
    /// 64. It is the path every player and gateway uses,
    /// `/<name>/index.m3u8`, so it never changes once viewers have it. The
    /// show's id is made from it.
    pub name: String,
    /// What the screen and the show list call it. Left out: the name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// False: kept, and its show stopped.
    #[serde(default = "yes")]
    pub enabled: bool,
    pub input: ChannelInput,
    /// A profile's id from `headend.channel.list` (`copy` passes the input
    /// through as it comes), or `custom` with `custom` set.
    #[serde(default = "copy")]
    pub profile: String,
    /// The conversion, when `profile` is `custom`. A Flussonic `transcoder`
    /// line is imported as one of these.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom: Option<TranscodeProfile>,
    #[serde(default)]
    pub hls: ChannelHls,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<ChannelOutput>,
    /// Where it came from: `added here`, `import of <source>`, or
    /// `<file>, edited here`. Set by the station; ignored when sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

fn yes() -> bool {
    true
}
fn copy() -> String {
    "copy".into()
}

/// A channel as `headend.channel.list` answers it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelEntry {
    pub channel: HeadendChannel,
    /// The show it runs as, once made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show: Option<String>,
    /// Set for a stream of the Flussonic config this station follows: the
    /// file. An edit here takes the channel out of the file's hands (see
    /// `headend.channel.update`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follows: Option<String>,
    /// Set for a channel that came from the followed file and was edited
    /// here: the file it no longer follows. `headend.channel.follow` hands
    /// it back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detached_from: Option<String>,
}

/// An encoder this machine has, for the profile editor's choice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HeadendEncoder {
    /// The catalogue's id, `h264-nvidia`, `x264`.
    pub id: String,
    /// `h264`, `h265`.
    pub codec: String,
    /// `gpu` or `cpu`.
    pub on: String,
}

/// `headend.channel.list`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HeadendSetup {
    /// Every channel: the ones kept here, then the followed file's.
    pub channels: Vec<ChannelEntry>,
    /// The built in profiles, then the ones saved here.
    pub profiles: Vec<TranscodeProfile>,
    /// The video encoders the planner may pick, GPU ones first.
    pub encoders: Vec<HeadendEncoder>,
    /// The Flussonic config this station follows, when it follows one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub following: Option<String>,
}

/// `headend.channel.add`: one channel or many, made in one go.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChannelAddRequest {
    pub channels: Vec<HeadendChannel>,
    /// True: check them and price them, and make nothing. Left out: false.
    #[serde(default)]
    pub dry_run: bool,
}

/// What an add, an update or a removal did.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelChange {
    /// The channels made, changed or removed, by name.
    pub done: Vec<String>,
    /// Each one that was not, with why. `index` is its place in the request.
    pub refused: Vec<ShowRefused>,
    pub dry_run: bool,
    /// Channels whose show had to be made again (input or conversion moved).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub restarted: Vec<String>,
}

/// `headend.channel.update`: one channel, replaced whole.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChannelUpdateRequest {
    /// The channel's name now.
    pub name: String,
    /// What it becomes. A different `name` renames it, which changes its
    /// public path and makes its show again.
    pub channel: HeadendChannel,
}

/// `headend.channel.bulk`: the same change to many channels.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChannelBulkRequest {
    pub names: Vec<String>,
    /// A profile for every one of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// With `profile` `custom`: the conversion every one of them takes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom: Option<TranscodeProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Join every multicast input on this card. Empty: the default route's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface: Option<String>,
}

/// `headend.channel.remove` and `headend.channel.follow`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChannelNames {
    pub names: Vec<String>,
}

/// `headend.export`: the channels kept here in Flussonic's config language.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HeadendExport {
    pub text: String,
    /// What the text cannot say, one line each: a profile setting Flussonic
    /// has no word for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}
