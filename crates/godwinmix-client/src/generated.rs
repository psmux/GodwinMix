// Generated from protocol.json by clients/gen/generate.py. Do not edit.
//
// Every type, method and event the core describes in `core.api`, as Rust. A
// method added to the core reaches this file by running:
//
//     python3 clients/gen/generate.py
//
// The drift test in tests/generated.rs fails if this file and protocol.json
// have parted company.
//
// String unions (a source state, an output state) are `String` aliases with a
// table of the values this api_level knows, not enums. A core one level ahead
// may send a state this build has never heard of, and a client that refuses to
// parse the whole status document because of one unknown word is worse than
// useless during a show.
#![allow(clippy::all)]

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

use crate::{Client, Result};

pub const API_LEVEL: u32 = 1;
pub const API_COMPATIBLE: u32 = 1;

/// What pressing the button does.
pub type ActionKind = String;
/// The values api_level 1 knows for [`ActionKind`].
pub const ACTION_KIND_VALUES: &[&str] = &["set-config", "install-plugin", "enable-plugin", "open", "retry", "restart"];

/// `adbreak.start`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AdBreakRequest {
    /// Programme running time to open the break on. Omit to roll now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at_running_time_ms: Option<u64>,
    /// Source to rejoin afterwards. Omit to return to whatever is on air.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_to: Option<String>,
    /// File path or URI of the clip to roll.
    pub uri: String,
}

/// An ad break, either armed for a future cue or currently on air.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AdStatus {
    /// False while it is prerolled and waiting for its cue.
    pub on_air: bool,
    /// Source to return to when the ad ends. None returns to the slate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_to: Option<String>,
    pub uri: String,
}

/// `channel.destination.add`. Send a channel's stream on to a platform.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AddDestinationRequest {
    /// On by default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// The channel.
    pub id: String,
    /// The stream key. Write only: no method reads it back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// What the list calls it. The platform's name when left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// youtube, facebook, twitch, custom or srt.
    pub platform: String,
    /// Convert the stream before sending it: `{"preset": "youtube-720p30"}`
    /// or a rendition request written out. Left out, or one the stream
    /// already matches, the stream is sent as it arrives.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendition: Option<RenditionChoice>,
    /// The ingest address. Left out, the platform's own; custom and srt need
    /// one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    /// Which of the channel's streams to send. `*`, the default, is the first
    /// one live.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>,
}

/// `filter.add`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AddFilterRequest {
    /// The name this filter answers to afterwards. A slug, and yours to pick.
    pub id: String,
    /// The filter's own settings. What goes in here is the filter's business,
    /// not the core's.
    pub params: BTreeMap<String, Value>,
    /// Filter the programme rather than one source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub programme: Option<bool>,
    /// `input` puts it before the proxy boundary, where the thumbnail sees it
    /// too; `programme` puts it on this source's programme branch only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
    /// The source to hang it on. Leave it out and set `programme` to filter
    /// everything that goes out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// A filter type id, as `plugin.list` and `core.api` `kinds` report them.
    #[serde(rename = "type")]
    pub r#type: String,
}

/// `scene.item.filter.add`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AddItemFilterRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
    pub item: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub params: BTreeMap<String, Value>,
    pub scene: String,
    /// A filter type id, as `plugin.list` reports them.
    #[serde(rename = "type")]
    pub r#type: String,
}

/// `scene.item.add`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AddItemRequest {
    /// What the item shows: `{"source": "cam1"}`, `{"ref": "<scene id>"}` or
    /// `{"graphic": "plugin/id"}`.
    pub content: Value,
    /// A draft id from `scene.edit.begin`, to change a working copy instead of
    /// the live document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
    /// What to call it. Left out, a source item is named after its source,
    /// because a model reasons about words.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub scene: String,
    /// Where it goes. Left out, the next free cell of a grid over what is
    /// already there, so a drop on a scene never needs a dialog.
    pub transform: Value,
}

/// `output.add`. The id and the URL are the whole of it for an RTMP
/// destination; anything else a kind understands rides in `params`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AddOutputRequest {
    /// Stable id for this destination.
    pub id: String,
    /// Reconnect policy: "own" retries quickly, for servers you run; "cdn"
    /// backs off harder, for platforms that penalise hammering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
    /// What to make from the programme for this destination: a rendition
    /// request or `{"preset": "youtube-720p30"}` (`rendition.presets` lists
    /// them). Absent means the programme encoder, at no extra cost.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendition: Option<BTreeMap<String, Value>>,
    /// rtmp:// or rtmps:// URL including the stream key. Left out for a
    /// kind with no address of its own, such as `hls/output`, which is
    /// served from the control port.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    /// Anything this build does not know a name for.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// `plugin.add`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AddPluginRequest {
    /// Where the plugin comes from. One of: `owner/repo` (a GitHub release,
    /// optionally `@version`), a git URL ending in `.git`, `cargo:name`,
    /// `npm:@scope/name`, `pypi:name`, `oci:ref`, an absolute path to a
    /// directory, or a bare plugin name to look up in the marketplaces.
    pub source: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AddSceneRequest {
    /// A colour for every client, the tally and the Stream Deck to agree on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub name: String,
}

/// `source.add`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AddSourceRequest {
    /// Stable id used by `program.take` and `source.remove`. Lowercase
    /// letters, digits and dashes. Derived from the name or the host when
    /// omitted, with a numeric suffix if that is taken.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// "web" renders the URL as a page in the browser sidecar, the same as
    /// writing `web+` in front of it. "auto" or omitted works the protocol out
    /// from the URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Name shown to an operator. Defaults to the host of the URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Websites only: "auto" lets the mixer decode the page's own video
    /// outside the browser and draw the page over the top, which saves about a
    /// CPU core. "off" is the default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superimpose: Option<String>,
    /// Stream URL, file path, or with kind "web" the address of a page.
    pub uri: String,
    /// Anything this build does not know a name for.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// `ext.agent`. `true` takes the default thresholds; an object moves them.
pub type AgentExt = Value;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentStateRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
}

/// One condition that holds now.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Alarm {
    /// One sentence for a person: what was measured, and against what.
    pub detail: String,
    pub kind: AlarmKind,
    /// Unix milliseconds when the condition began. For black, freeze and
    /// silence that is when the picture or sound first measured so, not when
    /// the alarm's duration ran out.
    pub since_ms: u64,
}

/// What an alarm is about.
pub type AlarmKind = String;
/// The values api_level 1 knows for [`AlarmKind`].
pub const ALARM_KIND_VALUES: &[&str] = &["no-input", "stall", "black", "freeze", "silence", "cc-errors", "loss", "output-failed", "governor-refused", "shed"];

/// A show's alarms, as a person sets them from the page. Left out fields
/// keep the measuring side's defaults; a duration of 0 switches that check
/// off.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AlarmSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub black_ms: Option<u64>,
    /// Whether black, freeze and silence are watched at all. Left out: on
    /// for a show without compositing, off for one that composites.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freeze_ms: Option<u64>,
    /// The peak level under which sound counts as quiet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub silence_dbfs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub silence_ms: Option<u64>,
}

/// The nine alignment keywords, used to place content inside its frame.
pub type Align = String;
/// The values api_level 1 knows for [`Align`].
pub const ALIGN_VALUES: &[&str] = &["top-left", "top-center", "top-right", "center-left", "center", "center-right", "bottom-left", "bottom-center", "bottom-right"];

/// When a change to a key takes effect.
pub type Applies = String;
/// The values api_level 1 knows for [`Applies`].
pub const APPLIES_VALUES: &[&str] = &["live", "next_source", "restart"];

/// `scene.apply_graphic`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ApplyGraphicRequest {
    /// Answer with a still of the armed scene as well as the records.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame: Option<bool>,
    /// The graphic to fill, `ograf/lower-third`.
    pub graphic: String,
    /// Which placement, by the name you gave the item or by its id. Left out,
    /// every placement of this graphic is filled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    /// Bring it on after filling it in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play: Option<bool>,
    /// Take it off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<bool>,
    /// The fields, by name. `scene.item.schema` says which there are.
    pub values: BTreeMap<String, Value>,
}

/// `scene.apply_layout`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ApplyLayoutRequest {
    /// How long the change takes, in milliseconds. 0 is a cut.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub easing: Option<String>,
    /// A layout name from `scene.layout.list`.
    pub layout: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The scene to apply it to. Left out, a new one is made. Applying onto an
    /// existing scene keeps the item ids, so an animated layout change is a
    /// property ramp rather than a cut.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    /// The layout's parameters by name: its source slots and its numbers.
    pub values: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ApplyRequest {
    /// Work out the plan and write nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
    /// Take the preset's value wherever the operator already has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force: Option<bool>,
    /// Leave the operator's sources and outputs alone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_sources: Option<bool>,
    /// A preset name, or a path to a directory holding `gmx-plugin.toml`.
    pub name: String,
}

/// What `preset.apply` answers with.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ApplyResult {
    /// Present when the preset was actually applied.
    pub applied: Value,
    /// True when nothing was written because `dry_run` was set.
    pub dry_run: bool,
    /// The sources and outputs this core picked up without a restart.
    pub live: Vec<String>,
    /// What still needs a restart, in plain words. Empty is the good case.
    pub needs_restart: Vec<String>,
    /// The whole plan, as JSON. The same object `preset.list` rows point at.
    pub plan: Value,
}

/// A file the collection carries with it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Asset {
    /// Relative to the collection root, always.
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

/// Whether the item's source is heard. A source is audible when any live item
/// of it says so, which is OBS's behaviour and changes no pad topology.
pub type Audio = String;
/// The values api_level 1 knows for [`Audio`].
pub const AUDIO_VALUES: &[&str] = &["follow", "always", "never"];

pub type AudioCodec = String;
/// The values api_level 1 knows for [`AudioCodec`].
pub const AUDIO_CODEC_VALUES: &[&str] = &["aac", "opus", "mp3", "ac3", "pcm", "other"];

/// `source.audio.set` takes an id as well as the levels: the id comes off the
/// path on REST and out of the params on `/rpc`, and both land in one object.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSetParams {
    /// The operator's fader for the whole source, 0.0 to 10.0. Omit to leave
    /// it where it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gain: Option<f64>,
    /// Source id.
    pub id: String,
    /// Gain per video underneath, by position. A null entry, or a list shorter
    /// than the number of videos, leaves those alone: `[null, 0.0]` silences
    /// the second video and touches nothing else.
    pub media: Vec<Option<f64>>,
    /// Mute the whole source. Held apart from the fader, so unmuting comes
    /// back to the level that was set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    /// Gain on a superimposed page's own sound.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<f64>,
}

/// A sound as it is.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioShape {
    /// 0 when unknown.
    pub bitrate_kbps: u32,
    pub channels: u8,
    pub codec: AudioCodec,
    pub sample_rate: u32,
}

/// The audio an output wants. Every field left out is taken from the source.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioWant {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate_kbps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codec: Option<AudioCodec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackendInfo {
    pub audio_decoder: String,
    pub audio_encoder: String,
    pub hardware_accelerated: bool,
    pub video_decoder: String,
    pub video_encoder: String,
}

/// An input's backup: an input with no backup of its own.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<BTreeMap<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program: Option<u16>,
    pub uri: String,
}

/// `scene.item.bind`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BindRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
    pub item: String,
    /// An expression over the collection's params and `W`, `H`. An empty
    /// string takes the binding off.
    pub param: String,
    /// A geometry path such as `frame.w` or `position.x`.
    pub prop: String,
    pub scene: String,
}

/// OBS's blend enum, so an import carries across unchanged.
pub type Blend = String;
/// The values api_level 1 knows for [`Blend`].
pub const BLEND_VALUES: &[&str] = &["normal", "add", "screen", "multiply", "lighten", "darken", "subtract"];

/// How media crosses between a node and the core.
pub type BridgeTransport = String;
/// The values api_level 1 knows for [`BridgeTransport`].
pub const BRIDGE_TRANSPORT_VALUES: &[&str] = &["rtp", "srt", "whip"];

/// What a batch costs, priced by the governor without taking anything.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BulkPlan {
    /// The input every rendition was priced against, because an input's
    /// shape is known only once it arrives.
    pub assumed_input: String,
    /// Every rendition of the shows that fit, summed. Copies cost nothing.
    pub cost: Cost,
    /// Whether every show of the batch fits.
    pub fits: bool,
    /// What the machine has free now.
    pub have: Cost,
}

/// What an importer is told before it reads the document.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bundle {
    /// Every file carried, by the path inside the bundle.
    pub assets: Vec<BundleAsset>,
    /// The envelope version. See [`BUNDLE_VERSION`].
    pub bundle_version: u32,
    pub canvas: Canvas,
    /// The collection's own stable id, repeated here so a listing can be read
    /// without unpacking the document.
    pub id: Id,
    pub name: String,
    /// Every plugin this collection needs, with the version range that will
    /// do. An importer that has none of them still gets the geometry.
    pub requires: Vec<Requirement>,
    /// What could not be carried, one line each, so a partial export is
    /// visible rather than silent.
    pub skipped: Vec<String>,
    /// The build that wrote it, for a bug report.
    pub written_by: String,
}

/// One file carried in the bundle.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BundleAsset {
    /// The asset id in the document.
    pub id: Id,
    /// Relative to the bundle root, forward slashes. Never absolute: see the
    /// head of this module.
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

/// `governor.calibrate`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CalibrateRequest {
    /// Measure even though something is on air. The measurement takes a few
    /// seconds of every core and can cost what is on air frames.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirm: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CalibrateResult {
    pub started: bool,
}

/// The output raster. One per collection in this release; 11 section 1 leaves
/// room for several.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Canvas {
    pub fps: u32,
    pub height: u32,
    pub width: u32,
}

/// The canvas every source is scaled onto and every output leaves by.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CanvasInfo {
    pub fps: i32,
    pub height: i32,
    pub width: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CellAssignment {
    pub h: i32,
    pub index: u32,
    /// None means this cell is the program return.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub w: i32,
    pub x: i32,
    pub y: i32,
}

/// `channel.certificate.generate`: a self signed certificate.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CertificateGenerateRequest {
    /// Host names and addresses it is for. Defaults to this machine's
    /// address, `localhost` and `127.0.0.1`.
    pub names: Vec<String>,
}

/// The certificate RTMPS answers with. The private key never leaves the core.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CertificateInfo {
    /// When it was set, RFC 3339 in UTC.
    pub created: String,
    /// SHA-256 of the certificate, as colon separated hex, to compare with
    /// what an encoder shows.
    pub fingerprint: String,
    /// The names it was made for, for a self signed one.
    pub names: Vec<String>,
    /// `uploaded`, or `self_signed` for one the mixer made.
    pub source: String,
}

/// `channel.certificate.set`: a certificate and its private key, as PEM.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CertificateSetRequest {
    /// The certificate, and any chain after it, as PEM.
    pub cert: String,
    /// Its private key, as PEM.
    pub key: String,
}

/// One thing an import does or would do.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Change {
    /// `add`, `replace`, `remove`, `keep`, `rename`, `set`, `wait`, `skip` or `missing`.
    pub action: String,
    /// The id, key, scene name or clip name, as the file has it.
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// `setting`, `source`, `output`, `channel`, `scene` or `media`.
    pub part: String,
    /// The new id or name, for a rename.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

/// A named place encoders publish to, over every protocol it has switched on.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Channel {
    /// The RTMP application name: the path segment after the port.
    pub app: String,
    /// A stream that goes live becomes a mixer source by itself.
    pub auto_source: bool,
    /// Where the channel's streams are sent on to, with what each is doing.
    /// Changed by `channel.destination.*`.
    pub destinations: Vec<Destination>,
    /// Off turns every publisher away with a sentence saying so.
    pub enabled: bool,
    /// A slug, and never changes once the channel exists.
    pub id: String,
    pub key_mode: KeyMode,
    /// The keys as hints, never the key itself: a read token sees only these.
    pub keys: Vec<ChannelKey>,
    /// What a person calls it.
    pub name: String,
    /// The protocols it takes publishers over, besides RTMPS.
    pub protocols: Vec<ChannelProtocol>,
    pub publish: ChannelPublish,
    /// RTMPS, on a port of its own, when a person has turned it on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rtmps: Option<Rtmps>,
    /// Live streams, and streams that left while a scene still holds their
    /// source.
    pub streams: Vec<ChannelStream>,
}

/// `channel.add`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelAddRequest {
    /// Defaults to a slug of the name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_source: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_mode: Option<KeyMode>,
    pub name: String,
    /// Defaults to RTMP alone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocols: Option<Vec<ChannelProtocol>>,
}

/// What `channel.add` answers: the channel and its first key.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelAdded {
    pub channel: Channel,
    pub key: NewKey,
}

/// One key, as a list shows it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelKey {
    /// When it was made, RFC 3339 in UTC.
    pub created: String,
    /// The last four characters, so a person can tell two keys apart.
    pub hint: String,
    pub id: String,
    pub label: String,
}

/// `channel.key.add`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelKeyAddRequest {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// `channel.key.remove`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelKeyRemoveRequest {
    pub id: String,
    pub key: String,
}

/// `channel.key.reveal`: one key of one channel.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelKeyRevealRequest {
    pub id: String,
    pub key: String,
}

/// `channel.list`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelList {
    /// The certificate RTMPS answers with, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate: Option<CertificateInfo>,
    pub channels: Vec<Channel>,
    /// The addresses an encoder can reach this machine at, first one first.
    pub hosts: Vec<String>,
    /// Every listener a channel needs, open or not, and why: the ports this
    /// mixer has open for ingest, and the channels each is open for.
    pub listeners: Vec<Listener>,
    pub rtmp: RtmpInfo,
}

/// A way a publisher reaches a channel. RTMPS is `Rtmps`, set apart
/// because it has a port of its own.
pub type ChannelProtocol = String;
/// The values api_level 1 knows for [`ChannelProtocol`].
pub const CHANNEL_PROTOCOL_VALUES: &[&str] = &["rtmp", "srt", "whip"];

/// Where an encoder is pointed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelPublish {
    /// The same for every protocol the channel has on, RTMP first.
    pub addresses: Vec<PublishAddress>,
    /// `<server>/main?psk=<key>`, with `<key>` left for the person to fill.
    pub example: String,
    /// `rtmp://<first address>:<port>/<app>`.
    pub server: String,
}

/// What `channel.remove` answers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelRemoved {
    pub removed: String,
}

/// `channel.set`: only what is named moves.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelSetRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_source: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_mode: Option<KeyMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Which protocols it takes, as a whole list: `["rtmp", "srt"]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocols: Option<Vec<ChannelProtocol>>,
    /// RTMPS on or off, and its port.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rtmps: Option<Rtmps>,
}

/// One stream on a channel.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelStream {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<StreamAudio>,
    /// Whole GOPs readers of it have lost by falling behind, this session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dropped_gops: Option<u64>,
    /// The publisher's address.
    pub from: String,
    /// The id of the key that let it in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub name: String,
    /// How it arrived: `rtmp`, `rtmps`, `srt` or `whip`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    /// Where a mixer on this machine reads it: the listener's own port on
    /// loopback. Any show adds it as a source with `source.add {type:
    /// "ingest/rtmp", relay, stream: "<app>/<name>"}`, and every show that
    /// does reads the one stream the station received.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay: Option<String>,
    /// When it last went live, in milliseconds since 1970.
    pub since_ms: u64,
    /// The mixer source it feeds, when it feeds one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// `live`, or `idle` for one that left while a scene holds its source.
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<StreamVideo>,
}

/// One key this call changed, and when the change takes effect.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigChanged {
    pub applies: Applies,
    pub key: String,
    /// Said when something outside the file wins over it, such as `--bind`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigGetRequest {
    /// Only these dotted keys. Empty or absent is every key.
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigGetResult {
    pub keys: Vec<ConfigKey>,
    /// Every key whose new value waits for a restart, whichever keys were asked for.
    pub needs_restart: Vec<String>,
    /// The config file these values are read from and written to.
    pub path: String,
}

/// One setting as `config.get` reports it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigKey {
    pub applies: Applies,
    pub default: Value,
    /// Dotted, as in `program.video_bitrate_kbps`.
    pub key: String,
    /// What wins over the file for this key, when something does: `--bind`,
    /// or `GODWINMIX_TOKEN` in the core's environment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overridden_by: Option<String>,
    /// True when the file differs from what the running core uses and only a
    /// restart will close the gap.
    pub pending: bool,
    pub secret: bool,
    /// For a secret: whether one is set. The value itself is never sent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set: Option<bool>,
    /// `file` when the key is written in the config file, `default` when not.
    pub source: String,
    /// What the config file says, or the default when it says nothing. Always
    /// null for a secret.
    pub value: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigResetRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
    /// Dotted keys to take out of the config file, so their defaults apply.
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigSetRequest {
    /// Check everything and write nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
    /// Dotted key to new value: `{"program.video_bitrate_kbps": 4500}`. Null
    /// puts a key back to its default. For a secret, the sentinel
    /// `"__secret__"` means leave it as it is, and an empty string clears it.
    pub values: BTreeMap<String, Value>,
}

/// What `config.set` and `config.reset` answer with.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigSetResult {
    /// Keys from this call in force now.
    pub applied: Vec<String>,
    /// Every key this call changed, each with its `applies`.
    pub changed: Vec<ConfigChanged>,
    /// True when nothing was written because `dry_run` was set.
    pub dry_run: bool,
    /// Every key, from this call or an earlier one, whose new value waits for
    /// a restart. Empty is the good case.
    pub needs_restart: Vec<String>,
    /// Keys from this call every source added or rebuilt from now on uses.
    pub next_source: Vec<String>,
    /// The config file written to.
    pub path: String,
    /// Secrets sent back as the sentinel, so left as they were.
    pub unchanged: Vec<String>,
}

/// How the bytes leave. Decides which codecs are allowed: FLV carries H.264
/// (and HEVC and AV1 in enhanced RTMP), WebRTC wants VP8, VP9, H.264 or AV1.
pub type Container = String;
/// The values api_level 1 knows for [`Container`].
pub const CONTAINER_VALUES: &[&str] = &["flv", "mpeg-ts", "mp4-fragmented", "mkv", "hls", "ll-hls", "dash", "rtp", "webrtc"];

pub type ConversionPhase = String;
/// The values api_level 1 knows for [`ConversionPhase`].
pub const CONVERSION_PHASE_VALUES: &[&str] = &["running", "done", "failed"];

/// One conversion, in flight or remembered after it finished.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConversionState {
    /// Set only on `failed`, shown to the operator verbatim: "no AAC encoder
    /// available" is worth more than "conversion failed".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The `.web.mp4` name, on `done`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// 0.0 to 1.0, position over duration, both read off the pipeline.
    pub progress: f64,
    pub state: ConversionPhase,
}

/// `core.info`: what this core is, what it can do, and where its edges are.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CoreInfo {
    pub api_compatible: u32,
    pub api_level: u32,
    pub canvas: CanvasInfo,
    /// Always "godwinmix".
    pub core: String,
    /// Feature strings a client can branch on: multiview, snapshot, uploads,
    /// mcp, browser, exec-sources, rehearsal, tokens.
    pub features: Vec<String>,
    pub limits: Limits,
    /// True when the core was started with `--rehearsal`, which refuses
    /// `output.add` and accepts rehearsal tokens.
    pub rehearsal: bool,
    /// Whether `core.restart` brings this core back, so a page can decide
    /// between a Restart button and a sentence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart: Option<RestartInfo>,
    /// True when the core was started with `--supervised` (or
    /// `GODWINMIX_SUPERVISED=1`): a service manager, a container runtime or
    /// the desktop app starts it again after it exits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supervised: Option<bool>,
    /// HTTPS on the control port, when it is on. Absent when `[control.tls]`
    /// is off or the certificate could not be loaded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsInfo>,
    /// Present when the request carried a token the core recognises.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<TokenInfo>,
    /// What a surface should start with, when a preset chose it. Absent on a
    /// core no preset has been applied to, which is what puts the welcome
    /// panel up in the reference UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiDefaults>,
    /// The build's own version, as in Cargo.toml.
    pub version: String,
}

/// What running one piece of work costs, in units the governor adds up.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Cost {
    /// Thousandths of one CPU core. 1000 is one whole core.
    pub cpu_millicores: u32,
    /// Share of one hardware device, in thousandths of what it can do, when
    /// the work runs on one.
    pub device_millis: u32,
    /// Hardware encoder sessions held (consumer NVIDIA cards cap these).
    pub device_sessions: u32,
    /// Bytes per second out of the machine, in kbit/s.
    pub egress_kbps: u32,
    /// Resident memory the work adds, in MiB.
    pub memory_mib: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CpuUse {
    pub cores: u32,
    /// What a station's processes cost now, read when asked: its own, every
    /// show process and every plugin it started. Left out by a single
    /// process core and where another process's CPU cannot be read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measured_millicores: Option<u32>,
    pub room_millicores: u32,
    pub used_millicores: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CreateFromRequest {
    /// A layout name from `scene.layout.list`. Left out, the number of sources
    /// picks one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Source ids, in the order they should be laid out.
    pub sources: Vec<String>,
}

/// How much of the content's own pixels to trim, normalised 0 to 1 so it
/// survives a canvas change. vMix and CasparCG do this; OBS crops in pixels,
/// which is why an OBS collection moved from 1080p to 720p loses its crops.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Crop {
    pub bottom: f64,
    pub left: f64,
    pub right: f64,
    pub top: f64,
}

/// One destination as a client sees it. The key never appears: `has_key`
/// says whether there is one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Destination {
    pub enabled: bool,
    /// What went wrong last, in words a person can act on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub has_key: bool,
    /// A slug, unique within its channel: `youtube`, `youtube-2`.
    pub id: String,
    /// What is going out, over the last second.
    pub kbps: u32,
    pub label: String,
    /// What the plan gave it, while its stream is live.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<DestinationPlan>,
    /// A platform id from the table: youtube, facebook, twitch, custom, srt.
    pub platform: String,
    /// Where a player opens it, for an output this machine serves as HLS.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playback: Option<Playback>,
    /// Connections lost and made again since it was switched on.
    pub reconnects: u32,
    /// Why it is not sending what it asked for, and what would fit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refused: Option<DestinationRefusal>,
    /// What it asked to be converted to. Absent: sent as it arrives.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendition: Option<RenditionChoice>,
    /// Milliseconds since `state` last changed.
    pub since_ms: u64,
    pub state: DestinationState,
    /// Which of the channel's streams to send. `*` is the first live one.
    pub stream: String,
    /// The scheme, host and port, and nothing that could carry a key.
    pub uri_host: String,
}

/// Copied as it arrives, or converted.
pub type DestinationMode = String;
/// The values api_level 1 knows for [`DestinationMode`].
pub const DESTINATION_MODE_VALUES: &[&str] = &["copy", "transcode"];

/// The plan's answer for one destination.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DestinationPlan {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioShape>,
    /// The video encoder, `h264-videotoolbox`, and why that one. Absent for a
    /// copy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoder_reason: Option<String>,
    pub mode: DestinationMode,
    /// The plan's nodes this destination reads, so a page can show which
    /// work it shares with the channel's other destinations.
    pub nodes: Vec<String>,
    /// One sentence: "copied: the source's video goes out as it is", "encoded
    /// because the source is 1920x1080 and this output wants 1280x720".
    pub reason: String,
    /// The stream it was planned against, when the destination names `*`.
    pub stream: String,
    /// What goes out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<VideoShape>,
}

/// Why a destination that asked for a rendition is not sending, and what
/// would. `error` on the destination carries the same sentence.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DestinationRefusal {
    /// Renditions that would fit now, largest first.
    pub advice: Vec<RenditionAdvice>,
    /// `governor` (the machine has no room), `plan` (nothing here can make
    /// it), `shed` (it ran and was stopped to keep what is on air).
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub have: Option<Cost>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub need: Option<Cost>,
}

/// Where a destination has got to.
pub type DestinationState = String;
/// The values api_level 1 knows for [`DestinationState`].
pub const DESTINATION_STATE_VALUES: &[&str] = &["off", "waiting", "connecting", "live", "reconnecting", "failed"];

/// Use of one hardware device by a plan.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceTotal {
    pub millis: u32,
    pub sessions: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceUse {
    pub id: String,
    /// `videotoolbox`, `nvidia`, `va`.
    pub kind: String,
    pub room_millis: u32,
    /// Absent when the device showed no limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sessions_max: Option<u32>,
    pub sessions_used: u32,
    pub used_millis: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscoverAnswer {
    pub found: Vec<Found>,
}

/// `device.discover`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscoverRequest {
    /// How long to look, shared between the devices. Two seconds by default,
    /// four and a half at most, because no method blocks for five.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscoverRequest2 {
    /// How long to listen. Capped at 4.5 seconds, so the call stays inside the
    /// five second ceiling every method is held to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// `scene.edit.begin`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DraftRecord {
    /// Pass this as `draft` on any `scene.item.*` call to edit the copy.
    pub draft: String,
    /// True when the client asked to edit on air.
    pub live: bool,
    /// The scene it was taken from.
    pub scene: String,
    /// The scene as it stands, so the client has something to draw at once.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view: Option<SceneView>,
}

/// `scene.edit.apply` and `discard`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DraftRequest {
    pub draft: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DuplicateSceneRequest {
    /// What to call the copy. A name already in use gets a number after it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub scene: String,
}

/// `source.duplicate`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DuplicateSourceRequest {
    /// The source to copy.
    pub id: String,
    /// Name for the copy. The original's name and " copy" when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Id for the copy. Derived from its name when omitted, with a numeric
    /// suffix if that is taken.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_id: Option<String>,
}

/// `scene.edit.begin`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditBeginRequest {
    /// True to edit the scene that is on air as you go. The default is off
    /// air: the draft is applied on the next take or on an explicit apply.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live: Option<bool>,
    pub scene: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnrolRequest {
    /// Where the node is, for the record. The node always dials the core, so
    /// this is what `node.list` shows before it has.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// What the node will call itself. A slug: it goes in `place` and in the
    /// node's certificate.
    pub name: String,
    /// How long the token is good for. Default one hour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl_secs: Option<u64>,
}

/// One thing a client can offer as a button. `label` is the button's text,
/// `kind` says what pressing it does, and the other fields are the ones that
/// kind uses. Flat rather than an enum with data, so every generated client
/// reads every field.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ErrorAction {
    /// `retry`: how long to wait first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_ms: Option<u64>,
    /// `set-config`: what `config.get` says about the key: `live`,
    /// `next_source` or `restart`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applies: Option<String>,
    /// `open`: a dialog, such as `settings`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dialog: Option<String>,
    /// `set-config`: the dotted key. `open`: the setting to show.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub kind: ActionKind,
    /// Short, in the imperative, for a person: "Turn the multiview on".
    pub label: String,
    /// `install-plugin` and `enable-plugin`: the plugin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `open`: a panel by id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub panel: Option<String>,
    /// `set-config`: the value to send.
    pub value: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportRequest {
    /// Put the clips themselves in, as base64, rather than their names and
    /// sizes. Refused past 256 MB: copy the media folder instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_media: Option<bool>,
    /// Put stream keys, channel keys, destination addresses and the control
    /// token in the file. Off unless asked; admin scope either way.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_secrets: Option<bool>,
    /// What to call the project. Defaults to "GodwinMix project".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whatever the page wants back when the file is opened: its layout and
    /// its settings. Carried as it is.
    pub page: Value,
}

/// `scene.export`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportRequest2 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    /// `json` for the document alone, `zip` for a bundle with its assets, or
    /// `dir` for the same bundle unpacked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Where to write it, on the machine the mixer is running on. Required for
    /// `dir`. For `zip`, leaving it out hands the bytes back as base64.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// The `ext` table from 03 section 6.
///
/// Every key is off by default. A terminal UI takes meters and tally and
/// declines multiview; a Stream Deck takes tally only; an agent takes nothing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ext {
    /// `event/agent.state` when a threshold crosses or a state flips, with a
    /// snapshot URL. `true` takes the defaults from 09 section 5 item 12.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentExt>,
    /// `event/meters` at 10 per second.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meters: Option<bool>,
    /// The mosaic: binary frames and `event/multiview.layout`. `false` or
    /// omitted builds nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiview: Option<MultiviewExt>,
    /// `event/source.position` for seekable sources.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub positions: Option<bool>,
    /// The preview scene, composited in the multiview pipeline at mosaic size,
    /// or `"full"` for a full resolution preview compositor built while
    /// subscribed. See 11 section 3.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<PreviewExt>,
    /// `event/tally`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tally: Option<bool>,
    /// `event/telemetry`: a line of numbers per tick, at 1 to 10 per second.
    /// This is what turns the probes on; nothing measures until it is here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telemetry: Option<TelemetryExt>,
    /// Anything this build does not know a name for.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// One filter in an item's chain.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Filter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub params: Value,
    /// The plugin qualified provide id, for example `chroma/filter`.
    #[serde(rename = "type")]
    pub r#type: String,
}

/// `filter.remove`, and anything else that names one filter.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilterIdRequest {
    pub id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilterListing {
    pub filters: Vec<FilterRecord>,
}

/// One filter as the core reports it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilterRecord {
    pub id: String,
    pub side: String,
    /// Absent on a programme filter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(rename = "type")]
    pub r#type: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilterRemoved {
    pub removed: String,
}

/// A source filter that had to be copied onto each placement.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilterReport {
    pub filter: String,
    pub obs_type: String,
    /// The items it was copied onto, by their path in the document.
    pub placements: Vec<String>,
    pub source: String,
}

/// One thing the validator found.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Finding {
    /// A stable machine readable code, so a client can filter or translate.
    pub code: String,
    /// The numbers behind the message, for a client that draws them.
    pub detail: Value,
    /// The items involved, in the order the message names them.
    pub items: Vec<Id>,
    /// One sentence naming the state and the next step.
    pub message: String,
    /// The scene it is in, when it is about one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene: Option<Id>,
    pub severity: Severity,
}

/// How content fills its frame. SVG's vocabulary, which replaces OBS's seven
/// bounds types and maps onto `sizing-policy` on a `glvideomixer` pad.
pub type Fit = String;
/// The values api_level 1 knows for [`Fit`].
pub const FIT_VALUES: &[&str] = &["none", "contain", "cover", "stretch", "fit-width", "fit-height", "max"];

/// Content on the wire. The same four shapes as the tree, except that a group
/// names no children: they are records whose parent is the group.
pub type FlatContent = Value;

/// `event/flush`: the end of a batch. A client renders here and not before.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Flush {
    /// The sequence number of the last event in the batch.
    pub seq: u64,
}

/// One thing found on the network.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Found {
    /// `host:port`, ready to hand to `godwinmix node --core`.
    pub address: String,
    /// The bridge version it speaks.
    pub api: u32,
    /// The instance name, which is the node's name.
    pub name: String,
    /// `node` or `core`.
    pub role: String,
}

/// A frame rate as a fraction, so 29.97 is exact.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fps {
    pub den: u32,
    pub num: u32,
}

/// The rectangle an item is fitted into.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Frame {
    pub h: f64,
    pub w: f64,
}

/// One item's derived box.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Geometry {
    pub height: f64,
    pub item: Id,
    /// The item's own opacity multiplied by every group's above it.
    pub opacity: f64,
    /// The names from the top item down, so a message can say
    /// `corner / pulpit` rather than an id.
    pub path: String,
    /// Present for an item whose content is a source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub source_height: f64,
    /// The canvas, which is what the document knows about a source's own size
    /// until the mixer says otherwise. Named so a client that does know can
    /// tell the two apart.
    pub source_width: f64,
    pub width: f64,
    pub x: f64,
    pub y: f64,
}

/// `program.golive`: add the page, add the destination, take the page.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GoLiveRequest {
    /// Source id. Derived from the host when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Where to send the programme. Added as an output unless one already
    /// sends there. Omit to leave the outputs alone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rtmp: Option<String>,
    /// "auto" (the default) or "off". See `source.add`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superimpose: Option<String>,
    /// The page to put on air. Plain http(s); `web+` is added here.
    pub url: String,
}

/// What `program.golive` answers with.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GoLiveResult {
    /// The output that was created or reused, if one was asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// The source that was created or reused.
    pub source: String,
    /// Where the source is now. It goes to programme as soon as it is live.
    pub state: SourceState,
}

/// `governor.status`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GovernorStatus {
    /// Unix seconds of the calibration in use; absent before the first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calibrated_at: Option<u64>,
    /// True while a calibration is running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calibrating: Option<bool>,
    pub cpu: CpuUse,
    pub devices: Vec<DeviceUse>,
    pub egress_kbps: u32,
    /// The key the calibration is stored under.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// What arrives: every channel stream and every direct show's input,
    /// as last counted. Zero on a core with no station.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ingress_kbps: Option<u32>,
    pub shed: Vec<ShedNote>,
}

/// `scene.graphic.list`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphicListing {
    pub graphics: Vec<GraphicType>,
}

/// One graphic this core can place, as the catalogue has it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphicType {
    /// The `[provides.designer]` block, when the plugin wrote one: the icon
    /// for the add gallery, the UI schema, the default frame and the gizmos.
    pub designer: Value,
    /// The OGraf manifest's path inside the plugin, so a client can fetch it.
    pub manifest: String,
    pub ograf: Ograf,
    pub plugin: String,
    pub provide: String,
    /// The plugin qualified id an item's `content.graphic` names,
    /// `ograf/lower-third`.
    pub type_id: String,
}

/// `source.group`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GroupSourcesRequest {
    /// The tray folder to put them in. Null takes them out of the one they
    /// are in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub sources: Vec<String>,
}

/// Everything in the document that is not a scene or an item: the name, the
/// canvas, the collection's parameters, the transitions it carries, the assets
/// and the source labels.
///
/// It is not a record and it has no id, so it cannot be diffed the way the
/// tree is. It is carried whole, because it is small and because the
/// alternative is that a command touching only the header produces an empty
/// patch and is thrown away by `edit`, which is exactly what used to happen to
/// `scene.params.set`, `source.set` and `source.group`: all three answered with
/// the change and none of them kept it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Header {
    pub assets: BTreeMap<String, Value>,
    pub canvas: Canvas,
    pub name: String,
    pub params: Value,
    pub sources: BTreeMap<String, Value>,
    pub transitions: Vec<Transition2>,
}

/// The header as it was and as it is.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeaderChange {
    pub after: Header,
    pub before: Header,
}

/// A show's health.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Health {
    /// Every alarm that holds now, oldest first.
    pub alarms: Vec<Alarm>,
    pub state: HealthState,
}

/// `event/health`, from a show that composites, about itself. The station
/// sends it on to clients as `event/show.health` with the show's id.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HealthEvent {
    pub health: Health,
}

/// The one word a monitoring wall colours a row by.
pub type HealthState = String;
/// The values api_level 1 knows for [`HealthState`].
pub const HEALTH_STATE_VALUES: &[&str] = &["ok", "warning", "alarm", "off"];

/// `program.history`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryRequest {
    /// How many takes to return, newest first. At most 100.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// `scene.undo` and `scene.redo`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryStep {
    pub patch: Patch,
    pub redo: i64,
    /// How many steps are still on each stack, so a UI greys out a button.
    pub undo: i64,
}

/// An `hls://` output's params, as an `hls/output` takes them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HlsOutputParams {
    /// true: parts of 333 ms.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low_latency: Option<bool>,
    /// LL-HLS part, 0 for none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_ms: Option<u32>,
    /// 500 to 10000, default 2000.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment_ms: Option<u32>,
    /// 16 characters or more.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub viewer_key: Option<String>,
    /// Seconds kept, default 30.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<u32>,
}

/// A UUID in the hyphenated form. Minted ids are version 7 (time ordered); ids derived from a layout are version 8.
pub type Id = String;

/// An id on its own: `source.get`, `source.remove`, `output.remove`,
/// `output.reconnect`, `media.remove`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct IdRequest {
    pub id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportObsRequest {
    /// Add the sources the scenes draw, each through `source.add`. Left out,
    /// only the scenes are added and the answer carries a `[[sources]]` block
    /// in `config_toml` instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub add_sources: Option<bool>,
    /// The collection JSON itself, as text: what a page reads from the file
    /// the person picked. Give this or `path`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// The collection JSON exported from OBS (Scene Collection, Export), as a
    /// path on the machine the core is running on. Give this or `content`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportReport {
    /// The `[[sources]]` block for a config file. Only for an import that did
    /// not add the sources itself, which is what the command line wants.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_toml: Option<String>,
    /// OBS attaches a filter to a source, so a camera keyed in one scene is
    /// keyed in all of them. Here filters belong to the item, so a source
    /// filter is copied onto each placement and each copy is named here. This
    /// is the one thing an import changes the meaning of, so it is reported
    /// rather than left for somebody to find on air.
    pub filters_duplicated: Vec<FilterReport>,
    pub items: i64,
    /// The scenes that were added, by the names they ended up with.
    pub scenes: Vec<String>,
    /// What could not be brought across, and why, one line each.
    pub skipped: Vec<String>,
    /// Every OBS source and what became of it: carried across, needing a
    /// plugin that is not installed, or skipped with the reason.
    pub source_report: Vec<SourceReport>,
    /// The sources the collection needs, by id. Without `add_sources` they
    /// have to be added separately.
    pub sources: Vec<String>,
    /// With `add_sources`: the sources added to the mixer, by id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources_added: Option<Vec<String>>,
    /// With `add_sources`: the sources that were not added, each with why.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources_not_added: Option<Vec<SourceNotAdded>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportRequest {
    /// Answer with what would change and change nothing. True unless false is sent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
    /// The project: the object `project.export` answered with, or its text.
    pub file: Value,
    /// Also write the file's machine settings: addresses, folders, hardware.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<Mode>,
}

/// `scene.import`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportRequest2 {
    /// The bundle: a `.zip` or the directory it unpacks to, as a path on the
    /// machine the core is running on.
    pub path: String,
}

/// What `scene.import` answers with.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportedReport {
    /// Where the assets were written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets_at: Option<String>,
    /// What the bundle said about itself.
    pub bundle: Bundle,
    pub items: i64,
    /// Plugins the collection needs that this core has not got. The scenes
    /// still came across; those items will draw nothing until it does.
    pub missing_plugins: Vec<String>,
    /// Assets that did not come across, with the items that draw them. Empty
    /// when everything landed.
    pub relink: Vec<Relink>,
    /// The scenes that were added, by the names they ended up with.
    pub scenes: Vec<String>,
}

/// What a show without compositing takes in. A show that composites makes
/// its input its one source.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InputSpec {
    /// Switched to when the input stalls, and back when it returns.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<BackupInput>,
    /// Per transport: `interface` for multicast, `latency` for SRT,
    /// `passphrase`. Passed to the host as given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<BTreeMap<String, Value>>,
    /// The MPEG-TS program of a feed that carries several. Left out: the
    /// first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program: Option<u16>,
    /// `udp://@239.1.1.1:5000`, `srt://...`, `rtmp://host/app/key`,
    /// `rtsp://...`, `https://.../x.m3u8`, `file:///clip.ts`, `rist://...`,
    /// or a channel's stream, `channel:<app>/<stream>`.
    pub uri: String,
}

/// What the input is doing, as the host last counted it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InputStats {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_channels: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc_errors: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kbps: Option<u32>,
    /// Between the last two keyframes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyframe_ms: Option<u64>,
    /// Since the last frame arrived.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_frame_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub packets_lost: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
}

/// One running instance and its cost.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InstanceRecord {
    pub buffers_dropped: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_percent: Option<f64>,
    pub instance: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_latency_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    /// The plugin it belongs to. Carried on the instance as well as on the
    /// plugin, because `plugin.stats` is a flat list and a caller holding one
    /// row should not have to go back for the name.
    pub plugin: String,
    pub provide: String,
    pub restarts: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rss_bytes: Option<u64>,
    pub state: String,
}

pub type ItemEdge = String;
/// The values api_level 1 knows for [`ItemEdge`].
pub const ITEM_EDGE_VALUES: &[&str] = &["left", "right", "top", "bottom"];

/// `scene.item.filter.set` and `remove`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ItemFilterRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
    /// Turn a filter off without taking it out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// The filter's name, or its position in the item's chain from 0.
    pub filter: String,
    pub item: String,
    pub params: BTreeMap<String, Value>,
    pub scene: String,
}

/// An item's props, which is an `Item` with the children lifted out into their
/// own records.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ItemProps {
    pub audio: Audio,
    pub bind: BTreeMap<String, Value>,
    pub blend: Blend,
    pub content: FlatContent,
    pub crop: Crop,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enter: Option<ItemTransition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit: Option<ItemTransition>,
    pub filters: Vec<Filter>,
    pub locked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub opacity: f64,
    pub transform: Transform,
    pub visible: bool,
}

/// Anything that names one item.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ItemRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
    /// The item's name or its id.
    pub item: String,
    pub scene: String,
}

/// `scene.item.schema`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ItemSchemaRequest {
    /// The item type: a graphic id like `ograf/lower-third`, or a plugin
    /// provide like `camera/source`.
    #[serde(rename = "type")]
    pub r#type: String,
}

/// One way on or off the canvas for one item.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ItemTransition {
    /// How long it takes, in milliseconds. 300 by default, ten seconds at
    /// most.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
    /// linear, ease-in, ease-out or ease-in-out (the default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub easing: Option<String>,
    /// For slide and wipe, the canvas edge it comes in from or goes out to:
    /// left (the default), right, top or bottom.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<ItemEdge>,
    /// Also play it when a scene holding this item is taken, in place of the
    /// scene's own transition for this item.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_take: Option<bool>,
    /// cut, fade, slide, zoom or wipe.
    #[serde(rename = "type")]
    pub r#type: ItemTransitionKind,
}

pub type ItemTransitionKind = String;
/// The values api_level 1 knows for [`ItemTransitionKind`].
pub const ITEM_TRANSITION_KIND_VALUES: &[&str] = &["cut", "fade", "slide", "zoom", "wipe"];

/// `scene.item.align`, `distribute`, `fit_to_canvas`, `cover_canvas`,
/// `arrange_grid`, `match_size`, `group`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ItemsRequest {
    /// `distribute`: horizontal or vertical.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub axis: Option<String>,
    /// `arrange_grid`: how many columns.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cols: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub easing: Option<String>,
    /// `align`: left, right, top, bottom, center-x, center-y.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<String>,
    /// Item names or ids.
    pub items: Vec<String>,
    /// `group`: what to call the group.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub scene: String,
    /// A client's own sequence number, echoed on the patch. See
    /// `SetItemRequest::seq`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    /// `match_size`: the item to match.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

/// What `channel.key.add` answers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyAdded {
    pub key: NewKey,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyColor {
    /// "#rrggbb".
    pub color: String,
    /// "point" for the colour at a point, "green" or "blue" for a screen.
    pub found: String,
    /// For a screen, the share of the picture it covers, 0 to 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share: Option<f32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyColorRequest {
    /// "green" or "blue" to look for that screen only. Either when left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen: Option<String>,
    /// The source id.
    pub source: String,
    /// 0 to 1 across the source's picture. With `y`, the colour there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<f64>,
    /// 0 to 1 down the source's picture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<f64>,
}

/// How a publisher gives its key.
pub type KeyMode = String;
/// The values api_level 1 knows for [`KeyMode`].
pub const KEY_MODE_VALUES: &[&str] = &["query", "stream"];

/// What `channel.key.reveal` answers: the key itself, and nothing a list
/// would carry.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyRevealed {
    pub secret: String,
}

/// A custom ladder.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LadderRef {
    pub ladder: Vec<RenditionRequest>,
}

/// A scene's geometry, for copying onto another one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub canvas: Canvas,
    pub items: Vec<LayoutItem>,
    /// The scene it came from, for a message.
    pub scene: String,
}

/// `scene.layout.copy` and `paste`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutClipboardRequest {
    /// What `scene.layout.copy` answered with.
    pub layout: Value,
    /// `name` matches item names first and falls back to slot order; `order`
    /// uses slot order alone.
    #[serde(rename = "match")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#match: Option<String>,
    pub scene: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutInfo {
    /// What the layout calls its own scene, which is the nearest thing it has
    /// to a description.
    pub description: String,
    pub name: String,
    /// The whole JSON Schema, so a client renders an inspector from it.
    pub params: Value,
    /// The parameters that take a source id, in the order sources are poured
    /// into them.
    pub sources: Vec<String>,
}

/// One item's geometry: everything about where it sits and nothing about what
/// it shows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutItem {
    pub crop: Crop,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub opacity: f64,
    pub transform: Transform,
    pub visible: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutListing {
    pub layouts: Vec<LayoutInfo>,
}

/// The ceilings a client should plan against rather than discover by being
/// refused.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Limits {
    /// How many events the bus holds before a slow client is told to resync.
    pub event_queue: i64,
    /// Longest any method blocks before it answers. The tightest client
    /// default in the wild, so nothing here can time out a client that used
    /// its own.
    pub max_call_secs: u64,
    /// Loudest a fader can be asked for. Out of range is clamped to this
    /// rather than refused.
    pub max_gain: f64,
    /// Longest `idempotency_key` accepted, in bytes.
    pub max_idempotency_key_bytes: i64,
    /// Largest upload the media endpoint accepts, in bytes.
    pub max_upload_bytes: i64,
}

/// One listener a channel needs, and whether it is open now.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Listener {
    /// The channels it is open for. Empty when nothing needs it.
    pub because: Vec<String>,
    /// The last port of a range, for WebRTC media.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_port: Option<u16>,
    /// Bound to 127.0.0.1 only, so nothing off this machine reaches it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loopback: Option<bool>,
    pub open: bool,
    pub port: u16,
    /// Why it is not open although a channel wants it, and what to do.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// `rtmp`, `rtmps`, `srt`, `whip`, `webrtc` (the media ports WHIP
    /// sessions use) or `relay` (the RTMP port on the loopback alone, for the
    /// mixer's own sources, while no channel has RTMP on).
    pub protocol: String,
    /// `tcp` or `udp`.
    pub transport: String,
}

/// `log.gst`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LogGstRequest {
    /// `GST_DEBUG` spelling: `rtmp2src:6,rtpjitterbuffer:5`.
    pub categories: String,
    /// How long before it goes back down. A minute by default, which is long
    /// enough to reproduce a fault and short enough that a forgotten firehose
    /// stops on its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
}

/// What `log.gst` answers with.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LogGstResult {
    /// The categories actually raised, which is what the caller asked for
    /// with anything GStreamer does not know dropped.
    pub categories: Vec<String>,
    pub duration_secs: u64,
}

/// `log.set`. Name an instance or a target, not both.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LogSetRequest {
    /// A plugin instance: a source or an output id. Its lines carry the id,
    /// so raising this one raises only that camera.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    /// `off`, `error`, `warn`, `info`, `debug`, `trace`, or `default` to stop
    /// overriding this one.
    pub level: String,
    /// A module path prefix such as `godwinmix::mixer`. The longest match
    /// wins, so a more specific override still beats a broader one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// `scene.history.mark`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MarkRequest {
    /// What to call the group of changes that follows. Omit it to end the
    /// group, so the next change is its own undo step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_codec: Option<String>,
    /// Where a conversion of this file stands, None when none was asked for in
    /// this process's lifetime.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversion: Option<ConversionState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub converted_path: Option<String>,
    /// None when the file could not be inspected; it is still listed, because
    /// an operator would rather see a clip they cannot read the length of than
    /// wonder why it is missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// The converted copy's absolute path when one exists on disk. This is
    /// what "add as source" should prefer over `path`.
    /// Whether the moov atom comes first (a player can start before the whole
    /// file arrives). None for a non-ISO container, never a reason to convert
    /// on its own. See `convert::moov_first`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub faststart: Option<bool>,
    pub has_audio: bool,
    pub has_video: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// Name shown in the UI, relative to the library root.
    pub name: String,
    /// Absolute path, which is what gets handed back to the ad break API.
    pub path: String,
    /// Why it is not, in words an operator can act on. Empty when it is.
    pub reasons: Vec<String>,
    pub size_bytes: u64,
    /// Short codec name of the first video stream ("h264", "vp9"), None when
    /// there is no video or it could not be inspected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_codec: Option<String>,
    /// True when the file needs no conversion to play in a browser: H.264 plus
    /// AAC (or no audio) in an MP4. See `convert::web_safety`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_safe: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaListing {
    /// True when the folder was not there and this listing made it, so a
    /// client can say "made the media folder" once instead of nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<bool>,
    pub dir: String,
    /// Set when the directory itself could not be read, so the UI can say why
    /// the list is empty instead of just showing nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub items: Vec<MediaItem>,
}

/// `event/meters`: the programme bus and every source, in one message at 10
/// per second, rather than one message per meter as the legacy stream sends.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Meters {
    /// Peak dBFS per channel on the programme bus.
    pub program: Vec<f64>,
    /// Peak dBFS per channel, per source id.
    pub sources: BTreeMap<String, Value>,
}

/// `source.missing`: the ids a scene draws, or none for every source the
/// mixer knows is not running.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MissingRequest {
    pub ids: Vec<String>,
}

/// One source that is not running, and what would bring it back.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MissingSource {
    /// The button that fixes it, when the error carries one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<ErrorAction>,
    /// The error it failed with, which names the next step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// True when `source.restore` can ask for it again.
    pub restore: bool,
    /// The kind, such as `camera/source`, when the mixer knows it.
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    pub why: MissingWhy,
}

/// Why a source is not running.
pub type MissingWhy = String;
/// The values api_level 1 knows for [`MissingWhy`].
pub const MISSING_WHY_VALUES: &[&str] = &["failed", "not_started", "removed", "unknown"];

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MixerStatus {
    /// Present while an ad break is armed or running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ad: Option<AdStatus>,
    pub backend: BackendInfo,
    pub multiview: MultiviewStatus,
    pub outputs: Vec<OutputStatus>,
    /// Source currently on program, or None while the slate is showing. A
    /// scene of one full canvas item reports that item's source here too, so
    /// anything written against this before scenes existed still reads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    /// Program pipeline running time. Cues are scheduled against this, not
    /// against wall clock, so a client can place a break on a known frame.
    pub running_time_ms: u64,
    /// The scene on air, when one was taken by name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    pub sources: Vec<SourceStatus>,
    pub uptime_secs: u64,
}

pub type Mode = String;
/// The values api_level 1 knows for [`Mode`].
pub const MODE_VALUES: &[&str] = &["replace", "merge"];

/// `scene.item.move` and `scene.item.copy`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MoveItemRequest {
    pub item: String,
    pub scene: String,
    /// The scene it is going to.
    pub to_scene: String,
}

/// `ext.multiview`. Accepts `false` to mean off, or an object.
pub type MultiviewExt = Value;

/// `event/multiview.layout`: how to read the binary frames that follow.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MultiviewLayout {
    pub cells: Vec<CellAssignment>,
    pub height: i32,
    /// Stable for as long as the cells are unchanged, and carried in the
    /// header of every frame, so a client that falls behind can tell which
    /// layout a late frame belongs to.
    pub id: u32,
    pub width: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MultiviewStatus {
    /// Cell index to source id, in reading order. Cell 0 is the program return
    /// when it is enabled.
    pub cells: Vec<CellAssignment>,
    pub cols: u32,
    pub enabled: bool,
    /// Frame rate of the mosaic, so the UI can size its own expectations.
    pub fps: i32,
    pub height: i32,
    pub rows: u32,
    pub width: i32,
}

/// `media.convert` and `media.remove` name a file rather than an id.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NameRequest {
    /// File name as it appears in the media listing. The REST layer puts it
    /// in the path, where the transform rule calls it `id`, so both spellings
    /// are read.
    pub name: String,
}

/// A key as it is made, with its secret. Afterwards only an admin gets the
/// secret again, one key at a time, from `channel.key.reveal`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NewKey {
    pub id: String,
    pub label: String,
    /// The key. A list never carries it; `channel.key.reveal` reads it back.
    pub secret: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NodeInstance {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub instance: String,
    pub latency_ms: u32,
    pub state: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NodeListing {
    /// Whether this core is listening for nodes at all.
    pub listening: bool,
    pub nodes: Vec<NodeView>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NodeName {
    /// The node's name, as it was enrolled.
    pub id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NodePlugin {
    pub name: String,
    pub provides: Vec<String>,
    pub version: String,
}

/// What `node.get` reports about one node, and what `node.list` reports about
/// all of them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NodeView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    pub clock_jitter_ms: f64,
    pub clock_offset_ms: f64,
    pub clock_synced: bool,
    /// Milliseconds since the last heartbeat. The same number
    /// `gmx_node_heartbeat_age_ms` carries.
    pub heartbeat_age_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    /// The instances it is hosting right now.
    pub instances: Vec<NodeInstance>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    /// The plugins this node has, name and version.
    pub plugins: Vec<NodePlugin>,
    /// The provide ids this node can run, `<plugin>/<provide>`.
    pub provides: Vec<String>,
    /// `online`, `offline`, or `expected` for a node listed in the config that
    /// has never dialled in.
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// The OGraf manifest, in the subset this host reads.
///
/// Everything else the file carries is kept in `rest` and passed on: OGraf is
/// an EBU specification that will grow, and a key this build has not heard of
/// is a key a newer client may want. Dropping it here would make the core the
/// thing that has to be upgraded first.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ograf {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The graphic's own id, as the OGraf file gives it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The module the web component is in, relative to the manifest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The JSON Schema of the graphic's own data. What the inspector renders
    /// and what `scene.apply_graphic` fills by name.
    pub schema: Value,
    /// How many steps `playAction` walks through. One means in and out.
    #[serde(rename = "stepCount")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_count: Option<u32>,
    #[serde(rename = "supportsNonRealTime")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_non_real_time: Option<bool>,
    #[serde(rename = "supportsRealTime")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_real_time: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

pub type OutputState = String;
/// The values api_level 1 knows for [`OutputState`].
pub const OUTPUT_STATE_VALUES: &[&str] = &["connecting", "live", "reconnecting", "failed"];

/// What one output is doing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputStats {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_millicores: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoder: Option<String>,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kbps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconnects: Option<u32>,
    /// `copy`, or what the plan gave it, such as `h264 1280x720`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendition_text: Option<String>,
    /// waiting, connecting, live, reconnecting, failed, or off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputStatus {
    /// False while the address still carries a placeholder a preset wrote in
    /// for somebody to replace, such as `YOUR-STREAM-KEY`. The key itself
    /// never leaves the core, so this is how a client knows to put its own
    /// form up and say "needs a stream key" without ever seeing the key.
    ///
    /// True for an address with no key in it at all, an SRT one for instance,
    /// because there is nothing there for anybody to replace.
    pub has_key: bool,
    pub id: String,
    /// Seconds of encoded data waiting in the pre-muxer queue. A number that
    /// climbs and stays high means the destination cannot keep up.
    pub queue_secs: f64,
    pub reconnects: u32,
    /// Per kind data from whatever plugin owns this output. Empty for the
    /// RTMP outputs the core builds itself.
    /// What this output asked to be made, when it asked: a rendition
    /// request or a preset. Absent means the programme encoder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendition: Option<RenditionChoice>,
    /// Why the governor has this output's rendition stopped just now, while
    /// it has. The output stays connected and resumes by itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shed: Option<String>,
    pub state: OutputState,
    pub uri_host: String,
    /// Anything this build does not know a name for.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// A sparse change to one item of a referenced scene.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Override {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crop: Option<Crop>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
    pub params: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
}

/// `scene.params.set`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParamsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    pub values: BTreeMap<String, Value>,
}

/// What changed in one transaction.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Patch {
    pub added: Vec<Record>,
    /// The client's own sequence number, echoed back.
    ///
    /// A drag cannot wait for a round trip, so the client kit draws the move
    /// itself and reconciles when the echo arrives. Without this it cannot
    /// tell an echo of the move it has already drawn past from a correction,
    /// and the handle rubber bands backwards under the cursor. Every geometry
    /// command carries a `seq`; this is that number coming back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_seq: Option<u64>,
    /// The collection's own properties, when they changed. Absent for the
    /// ordinary case, which is every command that moves an item.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<HeaderChange>,
    /// What the client called this change, for a label in an undo menu.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub removed: Vec<Id>,
    /// `document` today. `presence` (who is looking at what) is the other
    /// scope 11 section 4 names and is not implemented.
    pub scope: String,
    /// Monotonic, per core. A client that sees a gap asks for a fresh
    /// snapshot rather than guessing.
    pub seq: u64,
    /// Whoever asked for the change, so a client can suppress the echo of its
    /// own edits and not fight its own optimistic drawing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_client: Option<String>,
    pub updated: Vec<Update>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PathCreateRequest {
    /// The new folder's name: one folder, no separators, not hidden.
    pub name: String,
    /// The folder to make it in, as `path.list` names it.
    pub parent: String,
}

/// One folder inside the one listed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PathEntry {
    pub name: String,
    /// Absolute, ready to pass back as `path`.
    pub path: String,
    /// Whether the mixer can write into it.
    pub writable: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PathListRequest {
    /// The folder to list. Absent, empty or `~` is the mixer's home folder; a
    /// relative path is taken from there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// What `path.list` and `path.create` answer with.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PathListing {
    /// The folders in it, sorted by name. Files are never listed.
    pub dirs: Vec<PathEntry>,
    /// One level up, or null at the top of a root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// The folder listed, absolute and with links resolved.
    pub path: String,
    pub roots: Vec<PathRoot>,
    /// True when there were more than 500 and the rest were left out.
    pub truncated: bool,
    /// Whether the mixer can write into this folder.
    pub writable: bool,
}

/// A place the picker may start from.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PathRoot {
    /// `Home`, `Media folder` or `Config folder`.
    pub label: String,
    pub path: String,
}

/// What `pipeline.dot` answers with on `/rpc`. The REST route serves the same
/// graph as `text/vnd.graphviz`, so `gmx dot | dot -Tsvg` needs no unwrapping.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PipelineDot {
    /// The graph itself, in the dot language.
    pub dot: String,
    pub pipeline: String,
}

/// Which pipeline to look at. A source id, an output id, `programme` or
/// `multiview`. `pipeline.list` says what is running.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PipelineRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Where an instance runs: core, in-process, sidecar, or node:<name>.
pub type Place = String;

/// One node of a plan, as the page draws it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanNode {
    pub cost: Cost,
    /// The catalogue id of the encoder, on an encode node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoder: Option<String>,
    /// Stable across plans: `encode:programme:h264:1280x720p30:2800k:g2000`.
    pub id: String,
    /// `source`, `copy`, `decode`, `scale`, `encode`, `audio-convert`,
    /// `audio-encode`, `mux`.
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<PlanReason>,
    /// The outputs it works for, each once (for a channel's plan, the
    /// destination ids), however many rungs of one ladder it serves.
    pub serves: Vec<String>,
    /// Set while the governor has this node stopped to keep what is on air.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shed: Option<String>,
}

/// Why the planner decided what it did.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanReason {
    /// `hardware`, `software-only`, `device-full`, `shape-unsupported`,
    /// `copied`, `transcoded`.
    pub code: String,
    pub text: String,
}

/// `rendition.plan`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanRequest {
    /// `programme` (the default) or `channel:<id>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanTotals {
    pub cpu_millicores: u32,
    pub devices: BTreeMap<String, Value>,
    pub egress_kbps: u32,
}

/// `rendition.plan`, and the `plan` of `event/rendition.plan`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanView {
    pub nodes: Vec<PlanNode>,
    pub totals: PlanTotals,
}

/// The links of an output served as HLS from the control port, each with
/// the output's viewer key on it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Playback {
    /// The same segments as a DASH MPD.
    pub dash_url_path: String,
    /// `/hls/viewers/master.m3u8?show=bbc-one&key=...`.
    pub master_url_path: String,
    /// Players that fetched something in the last two windows.
    pub viewers: u32,
}

/// The whole of one plugin, for an agent about to use it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginDescription {
    pub description: String,
    pub enabled: bool,
    pub hooks: Vec<String>,
    /// Every running instance of it, with what it costs.
    pub instances: Vec<InstanceRecord>,
    /// The manifest as JSON, every table of it.
    pub manifest: Value,
    pub name: String,
    /// Why it is not loaded, when it is not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// The type ids it contributes: what goes in `type` on a source, an output
    /// or a filter.
    pub provides: Vec<String>,
    /// Where it is installed.
    pub root: String,
    /// Per provide id, its settings schema.
    pub schemas: BTreeMap<String, Value>,
    /// Per provide id, the description line from its SKILL.md.
    pub skills: BTreeMap<String, Value>,
    /// Where it was installed from, as it was typed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Its MCP tools, as `gmx_<plugin>_<tool>`. Reachable with `search_tools`;
    /// never in the hot list.
    pub tools: Vec<String>,
    /// What was checked about where this came from: "signed", "signed, digest
    /// only", or "custom, unreviewed". 06 section 4: an operator can only
    /// judge a plugin if the catalogue says what was checked.
    pub trust: String,
    /// The sentence behind the label.
    pub trust_detail: String,
    pub version: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginListing {
    pub plugins: Vec<PluginRecord>,
    /// Where plugins are read from on this machine.
    pub plugins_dir: String,
}

/// Anything that names one plugin.
///
/// The field is `id` because that is what the REST layer fills in from
/// `/api/v1/plugins/{id}`, and a plugin's id is its name: the namespace of
/// every id it contributes. `name` is accepted as well, for a JSON-RPC caller
/// who wrote the obvious thing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginName {
    pub id: String,
}

/// One plugin as the core reports it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginRecord {
    pub description: String,
    pub enabled: bool,
    pub hooks: Vec<String>,
    /// Every running instance of it, with what it costs.
    pub instances: Vec<InstanceRecord>,
    pub name: String,
    /// Why it is not loaded, when it is not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// The type ids it contributes: what goes in `type` on a source, an output
    /// or a filter.
    pub provides: Vec<String>,
    /// Where it is installed.
    pub root: String,
    /// Where it was installed from, as it was typed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Its MCP tools, as `gmx_<plugin>_<tool>`. Reachable with `search_tools`;
    /// never in the hot list.
    pub tools: Vec<String>,
    /// What was checked about where this came from: "signed", "signed, digest
    /// only", or "custom, unreviewed". 06 section 4: an operator can only
    /// judge a plugin if the catalogue says what was checked.
    pub trust: String,
    /// The sentence behind the label.
    pub trust_detail: String,
    pub version: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginRemoved {
    /// What went with it, so a caller can see the blast radius.
    pub provides: Vec<String>,
    pub removed: String,
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginSettings {
    pub name: String,
    /// The JSON Schema every surface renders, one per provide.
    pub schemas: BTreeMap<String, Value>,
    pub settings: BTreeMap<String, Value>,
}

/// What `plugin.update` answers with.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginUpdated {
    pub from: String,
    /// How long the new build took to answer `initialize`.
    pub handshake_ms: u64,
    pub plugin: PluginRecord,
    pub to: String,
}

/// A preset named by id.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PresetRef {
    pub preset: String,
}

/// `rendition.presets`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PresetsResult {
    pub presets: Vec<RenditionPreset>,
}

/// What `preview.close` answers with.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PreviewClosed {
    pub closed: bool,
    pub target: String,
}

/// `ext.preview`. Either `"full"`, `false`, or an object.
pub type PreviewExt = Value;

/// `scene.preview.frame`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PreviewFrameRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
}

/// `preview.open {target}`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PreviewOpenRequest {
    /// `program`, or a source id.
    pub target: String,
}

/// `scene.preview.set`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PreviewRequest {
    /// A draft from `scene.edit.begin` for the preview to draw in place of
    /// the armed scene, which is how a designer sees what it is laying out.
    /// What is armed is left as it is, and `scene` is ignored. An empty string
    /// goes back to the armed scene; so does applying or discarding the draft.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
    /// The scene to arm. Null or omitted disarms.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
}

/// What `preview.open` answers with.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PreviewSocket {
    /// The Unix socket to connect to, absolute. Read it with `unixfdsrc` in
    /// GStreamer, or with the media contract's own reader.
    pub path: String,
    pub target: String,
    /// What is on the far end, so a client knows what to expect before it
    /// connects.
    pub transport: String,
}

/// What `program.get` answers with, and what `program.take` returns so that no
/// follow up read is needed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProgramState {
    /// Present while an ad break is armed or on air.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ad: Option<AdStatus>,
    /// Sources the scene on air draws that this mixer does not have. The take
    /// went ahead without them and they draw nothing, so the slate or whatever
    /// sits under them shows through, until they are added back. Left out when
    /// every source is here.
    pub missing: Vec<String>,
    /// The scene armed for the next `program.take` with no argument.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    /// The previous source, which is what `program.revert` would take back to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
    /// Source on air, or null for the slate. A scene of one full canvas item
    /// reports that item's source here too, so anything written against this
    /// before scenes existed still reads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    /// Programme pipeline running time, in milliseconds.
    pub running_time_ms: u64,
    /// The scene on air, when one was taken by name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
}

/// Where an encoder is pointed for one protocol.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PublishAddress {
    /// The whole address with `<key>` where the key goes.
    pub example: String,
    /// `rtmp`, `rtmps`, `srt` or `whip`.
    pub protocol: String,
    /// The address without the key: `srt://10.0.0.5:9000`.
    pub server: String,
}

/// One scene or one item.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Record {
    pub id: Id,
    /// A fractional key. Siblings sort by it; see `order.rs`.
    pub order: String,
    /// The scene this item is in, or the group item it is a child of. Absent
    /// for a scene, which hangs off the document itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<Id>,
}

/// One asset an import could not put back.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Relink {
    pub asset: Id,
    /// The items that draw it, by scene and item name, so the person fixing
    /// it knows what will be blank until they do.
    pub items: Vec<String>,
    /// The path the document asks for.
    pub path: String,
    /// Why it could not be used: missing, or a hash that does not match.
    pub reason: String,
}

/// `channel.destination.remove`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RemoveDestinationRequest {
    /// The destination's id within the channel.
    pub destination: String,
    /// The channel.
    pub id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenameSceneRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub scene: String,
}

/// One thing a refused rendition could be instead, as a button.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenditionAdvice {
    /// Send this as the output's `rendition` to take the advice.
    pub request: RenditionRequest,
    /// "720p30 H.264 on h264-videotoolbox fits".
    pub text: String,
}

/// What an output asks for: a whole request, or a preset by id.
///
/// A request's `id` is replaced by the output's own id (a ladder's rungs get
/// `<output>-<rung>`), so a client may send any slug there.
pub type RenditionChoice = Value;

/// `event/rendition.plan`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenditionPlanEvent {
    pub plan: PlanView,
    pub scope: String,
}

/// One built in preset.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenditionPreset {
    /// Whether this machine can make it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available: Option<bool>,
    /// What the whole preset would cost here (every rung, the scaling and
    /// the sound), as the governor prices it on this machine.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<Cost>,
    /// `platform`, `ladder`, `audio` or `copy`, for grouping in a menu.
    pub group: String,
    /// `youtube-1080p30`, `abr-ladder-4`.
    pub id: String,
    /// Every rung, top first, for a ladder preset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ladder: Option<Vec<RenditionRequest>>,
    /// The one rendition, or the top rung of a ladder.
    pub request: RenditionRequest,
    /// What the page shows: "YouTube 1080p30".
    pub title: String,
    /// Why not, when it cannot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

/// What one output wants. A field left out means "whatever the source has",
/// so an empty request is a plain copy.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenditionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioWant>,
    /// How the bytes are wrapped on the way out. FLV when left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<Container>,
    /// Slug, unique within the show or channel that asks. Left out, the
    /// output's own id is used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Drop the audio altogether.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_audio: Option<bool>,
    /// Drop the video altogether (an audio only stream).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_video: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<VideoWant>,
}

/// `scene.item.reorder`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReorderRequest {
    /// Put it in front of this one. With neither, it goes to the front.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// Put it behind this one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
    pub item: String,
    pub scene: String,
    /// A client's own sequence number, echoed on the patch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
}

/// What `project.import` answers with.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Report {
    pub changes: Vec<Change>,
    pub dry_run: bool,
    /// What was tried and refused, each with the reason.
    pub failed: Vec<String>,
    pub name: String,
    /// Settings written to the file that take effect on the next start.
    pub needs_restart: Vec<String>,
    /// The page part of the file, for the page to put back.
    pub page: Value,
    /// What a person still has to do: a key to give again, a clip to copy.
    pub waiting: Vec<String>,
    pub written_by: String,
}

/// One plugin the collection needs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Requirement {
    /// The plugin name, `ograf`.
    pub plugin: String,
    /// The provide ids used, `ograf/lower-third`, so a reader can see what the
    /// collection actually asks the plugin for.
    pub provides: Vec<String>,
    /// A semver range, `^0.2.0`, or `*` when the exporter had no version to
    /// name because the plugin was not installed where the export ran.
    pub versions: String,
}

pub type ResponseFormat = String;
/// The values api_level 1 knows for [`ResponseFormat`].
pub const RESPONSE_FORMAT_VALUES: &[&str] = &["concise", "detailed"];

/// `core.restart`: what happened.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RestartAnswer {
    pub how: RestartHow,
    /// One sentence for a person: what happens now, or how to restart it.
    pub message: String,
    /// True when the core is on its way out and will be started again. False
    /// when nothing would start it again, in which case it keeps running.
    pub restarting: bool,
}

/// How a core that exits gets started again.
pub type RestartHow = String;
/// The values api_level 1 knows for [`RestartHow`].
pub const RESTART_HOW_VALUES: &[&str] = &["supervised", "none"];

/// `core.info.restart`: can this core be restarted from a client.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RestartInfo {
    pub how: RestartHow,
    /// True when `core.restart` will bring the core back by itself.
    pub possible: bool,
}

/// `event/resync`: the client fell behind and the stream has a hole in it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Resync {
    /// How many events were dropped.
    pub dropped: u64,
    /// The last sequence number the client is known to have. Everything after
    /// it was dropped; re-subscribe for a fresh snapshot.
    pub from_seq: u64,
}

/// The RTMP port every channel shares.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RtmpInfo {
    /// Whether the listener is running. False until the ingest plugin is
    /// installed and enabled.
    pub listening: bool,
    pub port: u16,
    /// Why not, and what to do, when it is not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// `rtmp://<address>:<port>` for each address this machine has.
    pub urls: Vec<String>,
}

/// RTMPS for one channel: off, or on at a port.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rtmps {
    pub enabled: bool,
    /// The port it listens on while enabled. 443 is offered first.
    pub port: u16,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SaveRequest {
    /// The new preset's name. A slug: lower case letters, digits and hyphens.
    pub name: String,
    /// Where to write it. Defaults to `~/.godwinmix/presets/<name>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out: Option<String>,
}

/// `scene.list`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SceneListing {
    pub scenes: Vec<SceneSummary>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SceneRemoved {
    pub removed: String,
}

/// Anything that names one scene.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SceneRequest {
    /// The scene's name or its id.
    pub scene: String,
}

/// What `scene.list` answers with per scene.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SceneSummary {
    /// True for the armed scene, which is the preview.
    pub armed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub id: Id,
    /// How many items, groups counted with their children.
    pub items: i64,
    pub name: String,
    /// Every source the scene draws, so a picker can grey out one whose
    /// sources are missing without reading the whole document.
    pub sources: Vec<String>,
}

/// One scene as a command answers with it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SceneView {
    pub canvas: Canvas,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// What `scene.validate` would say about it, so a client shows a warning
    /// without asking again.
    pub findings: Vec<Finding>,
    /// Where each item actually lands, after groups are flattened and
    /// references resolved. Bottom of the stack first, which is the order the
    /// compositor takes them in.
    pub geometry: Vec<Geometry>,
    pub id: Id,
    pub name: String,
    /// The scene's own record and one per item, parents before children.
    pub records: Vec<Record>,
}

/// `plugin.search`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchRequest {
    /// A word to look for in a plugin's name, description or kind. Empty
    /// lists everything.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub term: Option<String>,
}

/// One plugin a marketplace lists.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchResult {
    pub description: String,
    /// Whether it is already on this mixer.
    pub installed: bool,
    pub kinds: Vec<String>,
    pub marketplace: String,
    pub name: String,
    /// What to pass to `plugin.add`.
    pub source: String,
    /// custom, bronze, silver or gold. 06 section 4.
    pub tier: String,
    /// The newest listed version this core's api range can run.
    pub version: String,
}

/// What `plugin.search` answers with.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchResults {
    /// The marketplaces that were searched.
    pub marketplaces: Vec<String>,
    pub results: Vec<SearchResult>,
}

/// `source.seek`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SeekParams {
    /// Source id.
    pub id: String,
    /// Milliseconds from the start of the clip. Off either end is clamped
    /// rather than refused, so a scrubber flicked past the end lands there.
    pub position_ms: f64,
}

/// `core.session_log`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionLogRequest {
    /// How far back to read, in seconds. An hour by default, a day at most.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secs: Option<u64>,
}

/// `channel.destination.set`. Change one destination, naming only what moves.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SetDestinationRequest {
    /// The destination's id within the channel.
    pub destination: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// The channel.
    pub id: String,
    /// A new stream key. Left out keeps the one it has; an empty string
    /// clears it, where the platform allows none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// A new rendition. Left out keeps the one it has; `null` or
    /// `{"preset": "copy"}` goes back to sending the stream as it arrives.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendition: Option<RenditionChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>,
}

/// `filter.set`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SetFilterRequest {
    pub id: String,
    /// The settings to apply. Only the keys named are changed.
    pub params: BTreeMap<String, Value>,
}

/// `scene.item.set`: a state assignment. Only the keys named move.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SetItemRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
    /// How long to take getting there, in milliseconds. 0 is a cut.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// `linear` or `ease`. Only meaningful with a duration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub easing: Option<String>,
    pub item: String,
    /// Any of `name`, `transform`, `crop`, `opacity`, `blend`, `visible`,
    /// `locked`, `audio`, `content`, `enter`, `exit`. A key left out is left
    /// alone. `enter` and `exit` are `{type, edge, duration_ms, easing,
    /// on_take}` (type: cut, fade, slide, zoom, wipe), or null to clear one;
    /// an item hidden or shown on air plays them.
    pub props: BTreeMap<String, Value>,
    pub scene: String,
    /// A client's own sequence number, echoed on the patch so a drag can
    /// discard the echoes of moves it has already drawn past.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
}

/// `output.set`. Change one destination in place, naming only what moves.
///
/// The id picks the output and is never changed by this; renaming one is a
/// remove and an add, because the id is what alerts, hooks and the runtime
/// store call it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SetOutputRequest {
    /// The destination to change.
    pub id: String,
    /// "own" or "cdn", as `output.add`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
    /// Seconds of encoded data to hold before the muxer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_secs: Option<f64>,
    /// A new rendition, as `output.add` takes it. `null` puts the output
    /// back on the programme's own encode; left out keeps what it has.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendition: Option<BTreeMap<String, Value>>,
    /// The whole new address, stream key and all. Write only: no method ever
    /// reads it back, so leaving it out keeps the address already in force
    /// and a client can offer "change the buffer" without holding the key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    /// Anything this build does not know a name for.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// `plugin.settings.set`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SetSettingsRequest {
    pub id: String,
    /// Only the keys named are changed.
    pub settings: BTreeMap<String, Value>,
}

/// `source.set`: a full state assignment for one source.
///
/// Every field is optional and only what is named moves, which is how every
/// other setter in this protocol works. The one that matters here is `place`:
/// it moves a running source between the core, a sidecar and a node.
///
/// Unknown fields are refused rather than dropped. Serde's default is to
/// ignore what it does not recognise, and a setter that answers 200 to a field
/// it threw away is indistinguishable from one that saved it: the first party
/// drawer sent `uri` here for months and told the operator it was saved.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SetSourceRequest {
    /// The colour the UI and the tally show it in. On the scene document,
    /// like the name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Source id. `source` is accepted too, which is what the scene side of
    /// this method has always been called with.
    pub id: String,
    /// The latency budget in milliseconds, answered on the LATENCY query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u32>,
    /// What to call it in the UI. Kept on the scene document, so every
    /// client, the tally and an agent read the same name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Params for the source's own kind. Merged over what it has.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<BTreeMap<String, Value>>,
    /// Where it runs: `core`, `in-process`, `sidecar` or `node:<name>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place: Option<Place>,
    /// How a remote source's media travels: `rtp`, `srt` or `whip`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<BridgeTransport>,
}

/// How much the reader should care.
pub type Severity = String;
/// The values api_level 1 knows for [`Severity`].
pub const SEVERITY_VALUES: &[&str] = &["error", "warning", "info"];

pub type Severity2 = String;
/// The values api_level 1 knows for [`Severity2`].
pub const SEVERITY2_VALUES: &[&str] = &["critical", "info", "warning", "error"];

/// One thing the governor stopped or slowed, and why.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShedNote {
    /// "the 360p30 H.264 rendition for hls-main".
    pub what: String,
    /// The alert text.
    pub why: String,
}

/// One show, as `show.list` and `event/show.changed` carry it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Show {
    /// The alarms a person set for it, when they set any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alarms: Option<AlarmSettings>,
    /// true: scenes, transitions and a programme encode, in a process of
    /// its own. false: one input straight to its outputs, in the shared
    /// direct host, with no compositor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compositing: Option<bool>,
    /// Its process's CPU, in thousandths of one core, measured between two
    /// reads of `show.list`. Zero on the first read and while it is stopped.
    pub cpu_millicores: u32,
    /// Why it is not running, when it is not and a person did not ask.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<Health>,
    /// A slug: `main`, `second-room`.
    pub id: String,
    /// What it takes in. A show that composites makes it its one source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<InputSpec>,
    /// Its process's resident memory, in MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_mib: Option<u64>,
    pub name: String,
    /// The scene, or the source, on its programme. None while it shows the
    /// slate or is not running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_air: Option<String>,
    /// The outputs of a show without compositing. A show that composites
    /// keeps its outputs inside it, under `output.*` with `?show=<id>`.
    pub outputs: Vec<Destination>,
    /// What its outputs are sending, summed, in kilobits a second.
    pub programme_kbps: u64,
    /// How many times the station has started it again after it died.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restarts: Option<u32>,
    pub state: ShowState,
}

/// One show of `show.add_many`: what `show.add` takes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowAdd {
    /// Left out: true, a show with scenes and a programme, as before.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compositing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<ShowFrom>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<InputSpec>,
    pub name: String,
    pub outputs: Vec<ShowOutputSpec>,
}

/// `show.add_many`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowAddManyRequest {
    /// Left out: true. Says what would be made and what it would cost,
    /// and makes nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
    pub shows: Vec<ShowAdd>,
}

/// `show.add_many`'s answer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowAddManyResult {
    /// The ids made, or that would be made on a dry run.
    pub added: Vec<String>,
    pub dry_run: bool,
    pub plan: BulkPlan,
    pub refused: Vec<ShowRefused>,
}

/// `show.add`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowAddRequest {
    /// Left out: true, a show with scenes and a programme, as before.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compositing: Option<bool>,
    /// `"empty"` (the default), the id of a show to copy, or
    /// `{project: <file>}`. A copy takes the show's settings, sources and
    /// scenes, and leaves its outputs behind so nothing goes out twice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<ShowFrom>,
    /// What it takes in. Needed by a show without compositing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<InputSpec>,
    /// What a person calls it. The id is made from it.
    pub name: String,
    /// Where a show without compositing sends its input.
    pub outputs: Vec<ShowOutputSpec>,
}

/// `event/show.changed`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowChanged {
    pub show: Show,
}

/// What a new show starts from.
pub type ShowFrom = Value;

/// `event/show.health`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowHealthEvent {
    pub health: Health,
    pub id: String,
}

/// `show.list`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowList {
    /// The show a client reaches when it names none: the first one, which is
    /// the one the station was started with.
    pub current: String,
    pub shows: Vec<Show>,
}

/// `show.output.add`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowOutputAddRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// The show. `show` is taken as another name for it.
    pub id: String,
    /// Write only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The new output's own id, a slug. Made from the label when left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<HlsOutputParams>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendition: Option<RenditionChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

/// `show.output.remove`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowOutputRemoveRequest {
    /// The show. `show` is taken as another name for it.
    pub id: String,
    pub output: String,
}

/// `show.output.set`. Names only what moves.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowOutputSetRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// The show. `show` is taken as another name for it.
    pub id: String,
    /// A new key. Left out keeps the one it has.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The output's id.
    pub output: String,
    /// Replaces them all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<HlsOutputParams>,
    /// Left out keeps what it has; `null` or `{"preset": "copy"}` goes back
    /// to a copy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendition: Option<RenditionChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

/// An output as it is given to a show without compositing: an address, or
/// a platform and a key.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowOutputSpec {
    /// On by default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// A slug, unique within the show. Made from the label when left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// A platform's stream key. Write only: no method reads it back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// For an `hls://` output only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<HlsOutputParams>,
    /// youtube, facebook, twitch, custom or srt. Left out: custom, which
    /// takes a whole address in `uri`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    /// Left out: a copy of the input's own bytes, repackaged. Otherwise a
    /// rendition request or `{"preset": "youtube-720p30"}`, planned and
    /// admitted by the governor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendition: Option<RenditionChoice>,
    /// The whole address: `srt://10.0.0.9:9000`, `rtmp://host/app/key`,
    /// `udp://239.2.2.2:5000`, `hls://viewers`. For a platform, its ingest
    /// server when it is not the platform's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

/// A show of `show.add_many` that was not made, and why.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowRefused {
    pub data: Value,
    /// Its place in `shows`, from 0.
    pub index: i64,
    pub name: String,
    pub why: String,
}

/// `show.remove_many`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowRemoveManyRequest {
    pub ids: Vec<String>,
}

/// `show.remove_many`'s answer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowRemoveManyResult {
    /// Ids that were not removed, each with why.
    pub refused: Vec<ShowRefused>,
    pub removed: Vec<String>,
}

/// `show.remove`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowRemoved {
    pub removed: String,
}

/// `event/show.removed`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowRemovedEvent {
    pub id: String,
}

/// `show.rename`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowRenameRequest {
    pub id: String,
    pub name: String,
}

/// `show.set`. Names only what moves.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowSetRequest {
    /// Alarm settings; the fields named move, the rest stay.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alarms: Option<AlarmSettings>,
    /// true starts a show process whose one source is the input and moves
    /// the outputs to it; false goes back to a show without compositing,
    /// when it has one source and no scenes in use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compositing: Option<bool>,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<InputSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// `show.set`'s answer: the show, and what a switch of compositing did.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowSetResult {
    /// The alarms a person set for it, when they set any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alarms: Option<AlarmSettings>,
    /// true: scenes, transitions and a programme encode, in a process of
    /// its own. false: one input straight to its outputs, in the shared
    /// direct host, with no compositor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compositing: Option<bool>,
    /// Its process's CPU, in thousandths of one core, measured between two
    /// reads of `show.list`. Zero on the first read and while it is stopped.
    pub cpu_millicores: u32,
    /// Why it is not running, when it is not and a person did not ask.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<Health>,
    /// A slug: `main`, `second-room`.
    pub id: String,
    /// What it takes in. A show that composites makes it its one source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<InputSpec>,
    /// Its process's resident memory, in MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_mib: Option<u64>,
    pub name: String,
    /// The scene, or the source, on its programme. None while it shows the
    /// slate or is not running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_air: Option<String>,
    /// The outputs of a show without compositing. A show that composites
    /// keeps its outputs inside it, under `output.*` with `?show=<id>`.
    pub outputs: Vec<Destination>,
    /// What its outputs are sending, summed, in kilobits a second.
    pub programme_kbps: u64,
    /// How many times the station has started it again after it died.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restarts: Option<u32>,
    pub state: ShowState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switch: Option<SwitchReport>,
}

/// Where a show is in its life.
pub type ShowState = String;
/// The values api_level 1 knows for [`ShowState`].
pub const SHOW_STATE_VALUES: &[&str] = &["starting", "running", "stopped", "failed"];

/// One show's numbers, as `show.stats` answers them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowStats {
    /// What the show costs the machine, thousandths of a core: for a show
    /// that mixes, its process as last measured; for a direct show, its
    /// outputs' encodes summed. Left out while nothing has measured it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_millicores: Option<u32>,
    pub health: Health,
    pub id: String,
    /// None for a show with no input, or before the host has counted any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<InputStats>,
    pub outputs: Vec<OutputStats>,
    /// `mix`, `transcode` or `copy`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work: Option<ShowWork>,
}

/// `show.stats`'s answer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowStatsList {
    pub shows: Vec<ShowStats>,
}

/// `show.stats`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShowStatsRequest {
    /// Of `health`, `input` and `outputs`. Left out: all three.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<String>>,
    /// Left out: every show.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<String>>,
}

/// What a show does to make its outputs, which is what its load pays for.
pub type ShowWork = String;
/// The values api_level 1 knows for [`ShowWork`].
pub const SHOW_WORK_VALUES: &[&str] = &["mix", "transcode", "copy"];

/// `event/snapshot`: the full state, and where in the stream it sits.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Snapshot {
    pub seq: u64,
    pub state: MixerStatus,
}

/// `snapshot.get` on `/rpc` and through MCP. The REST route serves the same
/// bytes raw, because an `<img>` tag cannot read base64 out of JSON.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SnapshotRequest {
    /// Permit a width above the `[snapshot] max_width` ceiling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_large: Option<bool>,
    /// Ignore the per client rate limit for this one request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force: Option<bool>,
    /// "sheet", "program", or a source id. A `.jpg` on the end is accepted.
    pub id: String,
    /// Scale down to this many pixels across, keeping the aspect. Never
    /// enlarges. Omit for the `[snapshot] default_width` of 320, which is
    /// enough to see who is in shot; `width: 0` for the cell's own size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
}

/// What a source's audio controls read back as, which is what the audio
/// endpoint answers with.
///
/// Wider than `SourceAudio` because the fader and the mute apply to every
/// source, while the page and media balance belongs only to a superimposed one.
/// Every number here is read off the elements after the request landed, so a
/// request whose gain was clamped answers with the gain that took effect.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceAudioState {
    pub gain: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<Vec<f64>>,
    pub muted: bool,
    /// Absent on anything but a superimposed source, which is the only kind
    /// with separate sounds to balance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<f64>,
}

/// A source's name, colour and tray folder, as this collection has them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceMeta {
    /// Free text so a client can use whatever it draws with. Absent means the
    /// client picks one by kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// A tray folder: a tag on the source, purely for finding things. Not a
    /// scene group, which is a thing on the canvas.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// A source the import found and did not add, and why.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceNotAdded {
    pub id: String,
    /// The plugin that plays it, when that is what is missing, so a page can
    /// offer to install it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    pub reason: String,
}

/// Where a seekable source has got to, which is what the seek endpoint answers
/// with.
///
/// Both numbers are read back off the pipeline after the seek has landed, not
/// taken from the request. A seek snaps to a key unit, so the frame an operator
/// asked for and the frame they got are rarely the same millisecond, and a
/// scrubber drawn from the request would sit a little away from the picture.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourcePositionState {
    /// Absent while the demuxer has not worked the duration out yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    pub position_ms: u64,
}

/// One line of the report: an OBS source and what happened to it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceReport {
    pub obs_name: String,
    /// The OBS plugin type, for example `ffmpeg_source`.
    pub obs_type: String,
    /// How many items in the collection use it.
    pub placements: i64,
}

pub type SourceState = String;
/// The values api_level 1 knows for [`SourceState`].
pub const SOURCE_STATE_VALUES: &[&str] = &["connecting", "live", "stalled", "failed"];

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceStatus {
    /// Same for audio. `None` here while `has_audio` is true means the source
    /// advertised an audio track that never produced a decoded sample.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_idle_ms: Option<u64>,
    /// Index into the multiview grid, or None while the source has no cell.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// The operator's fader for this source, 0.0 silent through 1.0 unity to a
    /// ceiling of 10.0. Read back off the volume element rather than remembered,
    /// so what the UI shows is what the pipeline is doing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gain: Option<f64>,
    pub has_audio: bool,
    pub has_video: bool,
    pub id: String,
    /// Muted by the operator. Held apart from the fader so that unmuting returns
    /// the source to where it was rather than to unity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    pub name: String,
    /// Where this source has got to, and how long it runs, in milliseconds.
    /// `None` on anything not seekable, and on a seekable source whose duration
    /// the demuxer has not worked out yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position_ms: Option<u64>,
    /// True when this source can be scrubbed. A file can be. A camera, an RTMP
    /// feed or a page cannot, and asking one to is a mistake worth refusing
    /// rather than quietly doing nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seekable: Option<bool>,
    pub state: SourceState,
    pub uri: String,
    /// Milliseconds since the last video buffer, or None if none has arrived.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_idle_ms: Option<u64>,
    /// Anything this build does not know a name for.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StatsListing {
    pub instances: Vec<InstanceRecord>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StreamAudio {
    pub channels: u32,
    pub codec: String,
    pub kbps: u32,
    pub sample_rate: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StreamVideo {
    pub codec: String,
    pub fps: f64,
    pub height: u32,
    pub kbps: u32,
    pub width: u32,
}

/// `core.subscribe`: which events, and which expensive streams.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SubscribeRequest {
    /// Event name patterns, matched against the part after `event/`. `*`
    /// matches one or more characters: "program.*" matches `event/program.took`.
    /// An empty list subscribes to everything.
    pub events: Vec<String>,
    /// The expensive streams this client wants. Nothing here runs unless a
    /// client asks for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
    /// Which show this connection follows, on a station running several. The
    /// station opens the connection to that show from here on. Omitted means
    /// the show the URL named with `?show=`, or the first show.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show: Option<String>,
}

/// What `core.subscribe` answers with, before the snapshot arrives.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SubscribeResult {
    /// The event patterns now in force.
    pub events: Vec<String>,
    /// `ext` keys this build ignored. Empty on a build that knows them all.
    pub ignored_ext: Vec<String>,
    /// The sequence number the snapshot that follows is current as of.
    pub seq: u64,
}

/// What a switch of compositing did, in `show.set`'s answer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SwitchReport {
    /// What the show does now.
    pub compositing: bool,
    /// From the moment the outputs stopped where they were to the moment
    /// every one of them was live again where they went. None when they
    /// were not all live within the wait, or there were none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap_ms: Option<u64>,
    /// What a person should know: an output that was still connecting when
    /// the answer was sent, and so on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The outputs that moved.
    pub outputs: Vec<String>,
}

/// One take, as `program.history` reports it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TakeRecord {
    /// Programme running time the cut landed on.
    pub at_running_time_ms: u64,
    /// Token id that asked for it, or "core" when the mixer did it itself.
    pub by: String,
    /// Event sequence number the take was published under.
    pub seq: u64,
    /// What went on air. Null is the slate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// `program.take`: put a source on programme.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TakeRequest {
    /// Programme running time to land the cut on, in milliseconds. Omit for
    /// immediate. Read the current running time from `core.info` or a status
    /// snapshot first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at_running_time_ms: Option<u64>,
    /// The scene to take, by name or by id. `source` wins when both are
    /// given; with neither, the armed scene goes on air.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    /// Id of the source to put on air.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// "fade", or {type, duration_ms, params}. Absent is a cut.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition: Option<Transition>,
}

/// `event/tally`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tally {
    /// Source id to "program", "preview" or "off".
    pub sources: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskRequest {
    /// The id a long running method answered with. Spelled `id` on the REST
    /// route, where it is in the path, and `task_id` everywhere else, which
    /// is what 03 section 6 calls it.
    pub task_id: String,
}

pub type TaskState = String;
/// The values api_level 1 knows for [`TaskState`].
pub const TASK_STATE_VALUES: &[&str] = &["running", "completed", "failed", "cancelled"];

/// What `task.get` answers with.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskView {
    /// Seconds since the task was started.
    pub age_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The method that started it, so a client reading a list knows what it is
    /// looking at.
    pub kind: String,
    /// How long to wait before asking again, while it is still running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_interval_ms: Option<u64>,
    /// 0 to 1 where the work can say, absent where it cannot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    /// The body the method would have answered with, once it is done.
    pub result: Value,
    pub state: TaskState,
    pub task_id: String,
}

/// `ext.telemetry`. Accepts `false` to mean off, `true` for the default rate,
/// or an object naming it.
pub type TelemetryExt = Value;

/// `program.thumbnail`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThumbnailRequest {
    /// Pixels across, 16 to 320, made even. 320 when left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
}

/// `core.info.tls`: what the control port answers HTTPS with.
///
/// Enough for a page to say "open this address, accept the certificate
/// warning once, and check the fingerprint is this one".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TlsInfo {
    /// SHA-256 of the certificate, upper case hex in colon separated pairs,
    /// the way a browser's certificate viewer shows it.
    pub fingerprint: String,
    /// The host names and addresses a self signed certificate was made for.
    /// Empty for an operator's own certificate.
    pub names: Vec<String>,
    /// Where the certificate came from: `self_signed` (made by this mixer) or
    /// `files` (the operator's own, from `[control.tls] cert` and `key`).
    pub source: String,
    /// `https://` addresses this mixer can be opened at, the LAN one first.
    pub urls: Vec<String>,
}

/// What the calling token is allowed to do, echoed back so a surface can grey
/// out what it cannot reach instead of discovering it at the first refusal.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TokenInfo {
    /// "none" or "required": whether destructive calls need a confirm token.
    pub confirm: String,
    pub id: String,
    /// MCP tool profile this token is meant for: "standard" or "minimal".
    pub profile: String,
    pub rehearsal: bool,
    pub scopes: Vec<String>,
}

/// `tool.call`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolCallRequest {
    /// The tool's own arguments, as its input schema describes them.
    pub arguments: Value,
    /// `<plugin>/<tool>`, or the bare tool name when only one plugin has it.
    pub name: String,
}

/// Where an item sits and how it is sized.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Transform {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<Align>,
    /// Normalised 0 to 1 within the item's own box: (0,0) top left, (0.5,0.5)
    /// centre, (1,1) bottom right.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Vec2>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<Fit>,
    /// The rectangle the content is fitted into, in canvas pixels. Absent
    /// means the content's own size, scaled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame: Option<Frame>,
    /// Canvas pixels, of the item's anchor point.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Vec2>,
    /// Degrees, clockwise, about the anchor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<Vec2>,
}

/// A name, or an object.
pub type Transition = Value;

/// A named transition between two scenes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Transition2 {
    pub duration_ms: u32,
    pub id: Id,
    pub name: String,
    pub params: Value,
    #[serde(rename = "type")]
    pub r#type: String,
}

/// `program.transitions`: every transition a take may name on this core.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TransitionCatalogue {
    /// What a name on its own runs for, in milliseconds.
    pub default_duration_ms: u64,
    /// The directions `wipe`, `slide` and `push` take.
    pub directions: Vec<String>,
    /// The four easings every transition takes as `params.easing`.
    pub easings: Vec<String>,
    /// The edges an item transition takes.
    pub edges: Vec<String>,
    /// What an item's `enter` and `exit` may be.
    pub item_transitions: Vec<String>,
    /// The longest a transition may run, in milliseconds.
    pub max_duration_ms: u64,
    pub transitions: Vec<TransitionEntry>,
}

/// One name a take accepts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TransitionEntry {
    /// The duration a collection stores with it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    pub name: String,
    /// `built-in`, `collection` (a named transition the scene collection
    /// stores) or `plugin`.
    pub origin: String,
    /// The params it reads, for a built in one.
    pub params: Vec<String>,
    /// The type underneath a collection's name, which is the name itself for
    /// the other two.
    #[serde(rename = "type")]
    pub r#type: String,
}

/// How a take gets there. See docs/reference/transitions.md.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TransitionRequest {
    /// How long it takes. 0 is a cut.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// direction, easing, colour, clip... See list_transitions.
    pub params: BTreeMap<String, Value>,
    /// fade, wipe, slide, push, zoom, dip... See list_transitions.
    #[serde(rename = "type")]
    pub r#type: String,
}

/// What a surface starts with: the layout, the theme and the gallery mode.
///
/// Chosen by a preset (`preset.apply`), carried in `core.info` and pushed as
/// `event/ui.changed`. None of it changes what the core does. It exists so the
/// first page a volunteer sees is the one their preset chose rather than the
/// one the last person to use this browser chose. 05 section 3b is where the
/// four gallery modes are defined.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiDefaults {
    /// `live`, `snapshot`, `icon` or `label`. Absent means the surface asks
    /// the machine, which is what `gmx doctor` proposes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gallery: Option<String>,
    /// Slot to panels, top to bottom. Empty means the surface's own default.
    pub layout: BTreeMap<String, Value>,
    /// The preset that set these, so a surface knows one has been applied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// A theme id the surface resolves, for example `dark` or `calm`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
}

/// One record as it was and as it is.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Update {
    pub after: Record,
    pub before: Record,
}

/// `plugin.update`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdatePluginRequest {
    pub id: String,
    /// Where the new build comes from. Defaults to wherever this plugin was
    /// installed from last time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ValidateRequest {
    /// Leave it out to check the whole collection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
}

/// `scene.validate`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Validation {
    pub findings: Vec<Finding>,
    /// True when there is nothing to fix.
    pub ok: bool,
}

/// A point or a pair of factors.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Vec2 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<f64>,
}

pub type VideoCodec = String;
/// The values api_level 1 knows for [`VideoCodec`].
pub const VIDEO_CODEC_VALUES: &[&str] = &["other", "h264", "h265", "av1", "vp8", "vp9", "mpeg2", "prores"];

/// A picture as it is: codec, size, rate, bitrate.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoShape {
    /// Measured or configured. 0 when unknown (a raw source).
    pub bitrate_kbps: u32,
    pub codec: VideoCodec,
    pub fps: Fps,
    pub height: u32,
    /// 0 when unknown.
    pub keyframe_ms: u32,
    pub width: u32,
}

/// The video an output wants. Every field left out is taken from the source.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoWant {
    /// Target bitrate. A copy is kept when the source is within
    /// `bitrate_tolerance` of it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate_kbps: Option<u32>,
    /// Fraction either way a source's bitrate may differ and still be copied.
    /// 0.25 when left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate_tolerance: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codec: Option<VideoCodec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<Fps>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// Keyframe interval. Renditions in one ladder share it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyframe_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VirtualSetAnswer {
    /// Sources this call added for files it was given.
    pub added: Vec<String>,
    /// The key colour written on the presenter's key: "#rrggbb", or "auto"
    /// when no still of the camera could be had and the key finds it on air.
    pub key: String,
    /// "given", "guessed" or "auto".
    pub key_from: String,
    pub scene: SceneView,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VirtualSetRequest {
    /// What stands behind the presenter: a source id, a file name from the
    /// media library, or a path or URL to a picture or clip.
    pub background: String,
    /// A transparent picture in front of the presenter, drawn over the whole
    /// canvas: a source id, a media file name, or a path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreground: Option<String>,
    /// "auto" (the default) guesses the colour from the camera; "#rrggbb"
    /// gives it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// A source for the lower third area, in front of everything else.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lower_third: Option<String>,
    /// The new scene's name. "Virtual set" when left out, with a number after
    /// it when that is taken.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The camera to key: a source id.
    pub presenter: String,
    /// The presenter's picture as a fraction of the canvas, 0.3 to 1. 0.9
    /// when left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presenter_scale: Option<f64>,
    /// Where the presenter stands across the canvas, 0 to 1. 0.5, the
    /// middle, when left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presenter_x: Option<f64>,
}

/// `[vitals]`, and what `vitals.set` changes: the thresholds, and whether
/// to keep a mosaic up for the picture alarms while nobody is looking.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VitalsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alarms: Option<bool>,
    /// An 8 bit luma at or under which a pixel counts as black. 38 is ten
    /// percent of the way from video black (16) to white (235), the figure
    /// ffmpeg's blackdetect uses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub black_luma: Option<u8>,
    /// The share of pixels that must be black for the picture to be.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub black_ratio: Option<f64>,
    /// Seconds a picture must stay black before `black` is raised. 0: off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub black_secs: Option<f64>,
    /// Continuity errors within `window_secs` that raise `cc-errors`. 0: off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc_errors: Option<u64>,
    /// The mean luma difference between two samples, 0 to 1, under which the
    /// picture counts as unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freeze_diff: Option<f64>,
    /// Seconds a picture must stay unchanged before `freeze` is raised. 0: off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freeze_secs: Option<f64>,
    /// Packets lost within `window_secs` that raise `loss`. 0: off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loss: Option<u64>,
    /// The peak level in dBFS under which the sound counts as quiet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub silence_db: Option<f64>,
    /// Seconds the sound must stay quiet before `silence` is raised. 0: off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub silence_secs: Option<f64>,
    /// Seconds without a single packet of input before `stall` is raised.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stall_secs: Option<f64>,
    /// The window the two counters are judged over.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_secs: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProgramTookEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at_running_time_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition_id: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScenePatchEvent {
    pub added: Vec<BTreeMap<String, Value>>,
    /// The client's own sequence number, from the `seq` on the command, so a drag discards echoes of moves it has already drawn past.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_seq: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub removed: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
    /// Who asked for the change, so a client suppresses the echo of its own edits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_client: Option<String>,
    pub updated: Vec<Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PreviewChangedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceStateEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<SourceState>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourcePositionEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputStateEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconnects: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<OutputState>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AdbreakChangedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ad: Option<AdStatus>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiChangedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiDefaults>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HookBlockedEvent {
    /// The hook name, for example take.before.
    pub hook: String,
    /// The plugin that owns it, or the URL or command when it came from [[hooks]] in the config.
    pub plugin: String,
    /// What went wrong and what to do about it.
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaChangedEvent {
    pub conversion: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelChangedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<Channel>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelRemovedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelRefusedEvent {
    /// The publisher's address.
    pub from: String,
    /// The channel, or the application name asked for when no channel has it.
    pub id: String,
    /// The stream name asked for. Empty when the name was the key.
    pub stream: String,
    pub why: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<ErrorAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<Severity2>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TelemetryEvent {
    /// fraction of the picture at or below black, 0 to 1
    pub black: f64,
    pub freeze: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lufs_i: Option<f64>,
    /// short term loudness over three seconds, approximated from the programme meter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lufs_s: Option<f64>,
    /// how much the picture changed since the last frame, 0 to 1
    pub shot: f64,
    pub silence: bool,
    /// source id to 1 when it is live and 0 otherwise
    pub sources: BTreeMap<String, Value>,
    /// milliseconds since the Unix epoch
    pub ts: i64,
}

/// What a method is, for a surface that builds its own menu or its own REST call.
#[derive(Debug, Clone, Copy)]
pub struct MethodInfo {
    pub name: &'static str,
    pub summary: &'static str,
    pub scope: &'static str,
    pub mutating: bool,
    pub destructive: bool,
    pub rest: Option<(&'static str, &'static str)>,
}

pub const METHODS: [MethodInfo; 173] = [
    MethodInfo { name: "adbreak.end", summary: "Cut a running ad short, or disarm one that is scheduled.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/adbreak/end")) },
    MethodInfo { name: "adbreak.start", summary: "Interrupt the programme with a clip, then rejoin live when it ends.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/adbreak/start")) },
    MethodInfo { name: "agent.state", summary: "The compact document written for agents: the programme, each source's state and a motion score saying how much its picture is changing.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/agent/state")) },
    MethodInfo { name: "channel.add", summary: "Make a channel and its first key, which is in this answer. channel.key.reveal reads it again later.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/channels")) },
    MethodInfo { name: "channel.certificate.generate", summary: "Make a self signed certificate for RTMPS, for this machine's addresses unless names are given. Encoders must be told to accept it; one from a certificate authority needs no such step.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/channels/certificate/generate")) },
    MethodInfo { name: "channel.certificate.set", summary: "Give RTMPS a certificate: the PEM of the certificate (and its chain) and of its private key, as a certificate authority issued them. Checked before it is kept; the key is sealed and never read back.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/channels/certificate/set")) },
    MethodInfo { name: "channel.destination.add", summary: "Send a channel's stream on to YouTube, Facebook, Twitch, an RTMP server or an SRT receiver as it arrives. Nothing is decoded or encoded. The key is write only.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/channels/{id}/destination/add")) },
    MethodInfo { name: "channel.destination.remove", summary: "Stop sending a channel's stream to one destination and forget it. The publisher and the other destinations are not touched.", scope: "admin", mutating: true, destructive: true, rest: Some(("POST", "/api/v1/channels/{id}/destination/remove")) },
    MethodInfo { name: "channel.destination.set", summary: "Change one of a channel's destinations, naming only what moves: a new key, another server, which stream it sends, on or off. A key left out is kept.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/channels/{id}/destination")) },
    MethodInfo { name: "channel.get", summary: "One channel.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/channels/{id}")) },
    MethodInfo { name: "channel.key.add", summary: "Make another key for a channel, to give to one more person or encoder. The key is in this answer, and channel.key.reveal reads it again later.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/channels/{id}/key/add")) },
    MethodInfo { name: "channel.key.remove", summary: "Take one key back. A publisher on air with it is cut off and the next one is turned away; the other keys are untouched.", scope: "admin", mutating: true, destructive: true, rest: Some(("POST", "/api/v1/channels/{id}/key/remove")) },
    MethodInfo { name: "channel.key.reveal", summary: "Read one key of a channel back, to give it to an encoder again. Admin only; a list shows only the last four characters. Each read is logged with who asked, never with the key.", scope: "admin", mutating: false, destructive: false, rest: Some(("POST", "/api/v1/channels/{id}/key/reveal")) },
    MethodInfo { name: "channel.list", summary: "Every channel with its keys (as hints), the address to publish to over each protocol it has on, and what is live on it; and which ingest ports are open and for which channels.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/channels")) },
    MethodInfo { name: "channel.remove", summary: "Remove a channel and forget its keys. Sources it made that no scene holds go with it.", scope: "admin", mutating: true, destructive: true, rest: Some(("DELETE", "/api/v1/channels/{id}")) },
    MethodInfo { name: "channel.set", summary: "Rename a channel, switch it on or off, or change its application name, whether its streams become sources, how its key is given, which protocols it takes (rtmp, srt, whip) or RTMPS and its port. A port opens when the first channel needs it and closes when the last one stops. Only what is named moves.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/channels/{id}/set")) },
    MethodInfo { name: "codec.list", summary: "Every codec and element in the catalogue, which of them this machine actually has, and what it would pick.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/codecs")) },
    MethodInfo { name: "config.get", summary: "The mixer's settings: each key's value in the config file, its default, when a change to it takes effect, and which keys are waiting for a restart. Secrets say only whether one is set.", scope: "admin", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/config")) },
    MethodInfo { name: "config.reset", summary: "Put settings back to their defaults by taking them out of the config file. Answers like config.set.", scope: "admin", mutating: true, destructive: true, rest: Some(("POST", "/api/v1/config/reset")) },
    MethodInfo { name: "config.schema", summary: "Every setting config.set takes, as one JSON Schema: type, title, description, default, range or choices, and x-gmx-applies (live, next_source or restart).", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/config/schema")) },
    MethodInfo { name: "config.set", summary: "Change settings in the config file, keeping its comments. Every value is checked first and nothing is written unless all of them fit. Live keys take effect at once; the answer says which wait for the next source or a restart.", scope: "admin", mutating: true, destructive: true, rest: Some(("POST", "/api/v1/config/set")) },
    MethodInfo { name: "core.api", summary: "Every method, event and type as JSON Schema. The same document as protocol.json and `godwinmix --api-info`.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/core/api")) },
    MethodInfo { name: "core.doctor", summary: "The environment checks: GStreamer, the elements, the config, the disk and the ports. The same list `gmx doctor` prints.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/core/doctor")) },
    MethodInfo { name: "core.info", summary: "What this core is, what it can do, and where its edges are.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/core/info")) },
    MethodInfo { name: "core.restart", summary: "Stop the mixer and have it started again, when something will start it again. On a supervised core (core.info restart.possible) it answers restarting: true and exits; the programme is off air until it is back. On a core started by hand it answers restarting: false, says how to restart it, and keeps running.", scope: "admin", mutating: true, destructive: true, rest: Some(("POST", "/api/v1/core/restart")) },
    MethodInfo { name: "core.session_log", summary: "The append only record of everything that happened, back as far as you ask.", scope: "admin", mutating: true, destructive: false, rest: Some(("GET", "/api/v1/core/session_log")) },
    MethodInfo { name: "core.shutdown", summary: "Stop the mixer, and with it the programme. Nothing else takes the show off air, so this is deliberately its own call.", scope: "admin", mutating: true, destructive: true, rest: Some(("POST", "/api/v1/core/shutdown")) },
    MethodInfo { name: "core.startup_report", summary: "How long each stage of the start took, and what was over the 250 ms mark.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/core/startup_report")) },
    MethodInfo { name: "core.status", summary: "The full state: programme, every source, every output, the multiview grid, the encoder backend and any ad break.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/core/status")) },
    MethodInfo { name: "core.subscribe", summary: "Subscribe to the event stream. WebSocket only: the core answers event/snapshot then deltas, ending every batch with event/flush.", scope: "read", mutating: false, destructive: false, rest: None },
    MethodInfo { name: "device.discover", summary: "Ask every device plugin what it can see: cameras, NDI senders, publishers. Each candidate's params are ready for source.add.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/device/discover")) },
    MethodInfo { name: "filter.add", summary: "Hang a filter on one source or on the programme, live.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/filters")) },
    MethodInfo { name: "filter.list", summary: "Every filter in place, with what it is and where it sits.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/filters")) },
    MethodInfo { name: "filter.remove", summary: "Take a filter out of the pipeline.", scope: "operate", mutating: true, destructive: true, rest: Some(("DELETE", "/api/v1/filters/{id}")) },
    MethodInfo { name: "filter.set", summary: "Change a filter's settings in place. A filter that cannot take the change while running says so rather than being restarted behind your back.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/filters/{id}/set")) },
    MethodInfo { name: "governor.calibrate", summary: "Measure this machine's encoders again, in the background, a few seconds of every core. Refused while anything is on air unless `confirm` is true.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/governor/calibrate")) },
    MethodInfo { name: "governor.status", summary: "The resource governor: when this machine was measured, what is in use and free on the CPU and each GPU encoder, and what was shed to keep the programme whole.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/governor/status")) },
    MethodInfo { name: "log.gst", summary: "Raise GStreamer's own debug categories for a while, then let them fall back on their own.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/log/gst")) },
    MethodInfo { name: "log.levels", summary: "Every log level override in force, and the GStreamer categories still raised.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/log/levels")) },
    MethodInfo { name: "log.set", summary: "Change one instance's or one module's log level while the mixer runs.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/log/set")) },
    MethodInfo { name: "media.convert", summary: "Transcode a library file to a web safe copy, in the background.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/media/{id}/convert")) },
    MethodInfo { name: "media.list", summary: "The clips in the library, with durations and whether each has audio.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/media")) },
    MethodInfo { name: "media.remove", summary: "Delete a library file and its converted copy. Refused while it is a live source.", scope: "operate", mutating: true, destructive: true, rest: Some(("DELETE", "/api/v1/media/{id}")) },
    MethodInfo { name: "media.upload", summary: "Stream a file into the library. HTTP only: the body is the file.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/media/upload")) },
    MethodInfo { name: "node.discover", summary: "Look for nodes on the local network over mDNS. A network without multicast finds nothing and the [nodes] table in the config is the way there.", scope: "read", mutating: false, destructive: false, rest: Some(("POST", "/api/v1/nodes/{id}/discover")) },
    MethodInfo { name: "node.enrol", summary: "Mint a one time enrolment token for a node. The answer carries the command to run on the other machine. The token is good for one enrolment and expires.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/nodes/{id}/enrol")) },
    MethodInfo { name: "node.get", summary: "One node: its clock offset, how long since its last heartbeat, the plugins it has, and the instances it is hosting.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/nodes/{id}")) },
    MethodInfo { name: "node.list", summary: "Every node this core knows about: the ones connected now, the ones that have gone quiet, and the ones the config expects that have never dialled in.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/nodes")) },
    MethodInfo { name: "node.remove", summary: "Forget a node. Its bridge is closed, every token minted for a plugin on it is revoked, and its certificate stops working. Sources placed on it go to the slate until they are moved or the node enrols again.", scope: "admin", mutating: true, destructive: true, rest: Some(("DELETE", "/api/v1/nodes/{id}")) },
    MethodInfo { name: "output.add", summary: "Send the programme to another destination. The encoder is shared, so adding one costs nothing on air.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/outputs")) },
    MethodInfo { name: "output.get", summary: "One destination.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/outputs/{id}")) },
    MethodInfo { name: "output.list", summary: "Every destination, with its state, reconnect count and how much is buffered.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/outputs")) },
    MethodInfo { name: "output.reconnect", summary: "Drop and re-establish one destination's connection now, without waiting for its reconnect policy.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/outputs/{id}/reconnect")) },
    MethodInfo { name: "output.remove", summary: "Stop sending to a destination and forget it. Other outputs are unaffected.", scope: "operate", mutating: true, destructive: true, rest: Some(("DELETE", "/api/v1/outputs/{id}")) },
    MethodInfo { name: "output.set", summary: "Change a destination in place: a new address with a new stream key, a new reconnect policy, a deeper outage buffer. The address is write only, so a client that only wants the buffer never has to hold the key.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/outputs/{id}/set")) },
    MethodInfo { name: "path.create", summary: "Make one new folder inside a folder path.list shows, and list it. A folder that is already there is listed rather than refused.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/path/create")) },
    MethodInfo { name: "path.list", summary: "The folders in one folder on the mixer, and whether each is writable, for a folder picker. Only the home folder and the mixer's own folders are shown; files never are.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/path/list")) },
    MethodInfo { name: "pipeline.clock", summary: "The clock every pipeline is running against, and how far each one has got.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/pipeline/clock")) },
    MethodInfo { name: "pipeline.dot", summary: "One pipeline as a graphviz graph: every element, every pad and the caps negotiated between them.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/pipeline/dot")) },
    MethodInfo { name: "pipeline.latency", summary: "How much delay one pipeline is carrying, and which stage put it there.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/pipeline/latency")) },
    MethodInfo { name: "pipeline.list", summary: "Every pipeline running right now, by the name the other pipeline methods accept.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/pipeline/list")) },
    MethodInfo { name: "pipeline.queues", summary: "Every queue in one pipeline with how full it is, fullest first. A queue that stays full is where the trouble is.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/pipeline/queues")) },
    MethodInfo { name: "plugin.add", summary: "Install a plugin, while live, from any source form: a GitHub release (owner/repo), a git URL, cargo:, npm:, pypi:, a local directory, or a bare name looked up in the marketplaces this mixer knows. The signature and the api level are checked before anything is copied.", scope: "admin", mutating: true, destructive: true, rest: Some(("POST", "/api/v1/plugins")) },
    MethodInfo { name: "plugin.describe", summary: "One plugin in full: its manifest, the settings schema of every provide, and the description from each SKILL.md.", scope: "read", mutating: false, destructive: false, rest: Some(("POST", "/api/v1/plugins/{id}/describe")) },
    MethodInfo { name: "plugin.disable", summary: "Turn a plugin off without uninstalling it. It registers nothing and runs no process until it is enabled again.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/plugins/{id}/disable")) },
    MethodInfo { name: "plugin.enable", summary: "Turn a plugin back on. It registers what it declares and its instances start.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/plugins/{id}/enable")) },
    MethodInfo { name: "plugin.list", summary: "Every plugin installed, with what it provides and what each running instance is costing in cpu, memory, latency, dropped buffers and restarts.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/plugins")) },
    MethodInfo { name: "plugin.reload", summary: "Read a plugin's directory again and swap its running instances one at a time, with the freeze frame covering each.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/plugins/{id}/reload")) },
    MethodInfo { name: "plugin.remove", summary: "Uninstall a plugin and unwind everything it registered: its provides, its tools, its panels, its hooks and its discovery matchers.", scope: "admin", mutating: true, destructive: true, rest: Some(("DELETE", "/api/v1/plugins/{id}")) },
    MethodInfo { name: "plugin.search", summary: "Search every marketplace this mixer knows for a plugin, by name, description or kind. Answers what `gmx plugin add <name>` would install.", scope: "read", mutating: false, destructive: false, rest: Some(("POST", "/api/v1/plugins/{id}/search")) },
    MethodInfo { name: "plugin.settings.get", summary: "A plugin's settings as they stand, with its schema beside them.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/plugins/{id}/settings")) },
    MethodInfo { name: "plugin.settings.set", summary: "Change a plugin's settings. A plugin that cannot take a change while running says so rather than being restarted behind your back.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/plugins/{id}/settings")) },
    MethodInfo { name: "plugin.stats", summary: "Per instance cpu, memory, media latency, dropped buffers and restarts, refreshed once a second.", scope: "read", mutating: false, destructive: false, rest: Some(("POST", "/api/v1/plugins/{id}/stats")) },
    MethodInfo { name: "plugin.update", summary: "Fetch a newer build of a plugin, install it beside the one that is running, and prove it starts. A build that does not answer `initialize` within ten seconds is rolled back and the plugin that was working stays working.", scope: "admin", mutating: true, destructive: true, rest: Some(("POST", "/api/v1/plugins/{id}/update")) },
    MethodInfo { name: "preset.apply", summary: "Put a preset on this core: its config, its scenes, its layout, its theme and its gallery mode. Pass dry_run to get the plan and write nothing.", scope: "admin", mutating: true, destructive: true, rest: Some(("POST", "/api/v1/preset/apply")) },
    MethodInfo { name: "preset.list", summary: "Every preset this core can apply: the six built in, plus anything installed beside the binary or under ~/.godwinmix/presets.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/preset/list")) },
    MethodInfo { name: "preset.save", summary: "Turn this core's working setup into a preset directory somebody else can apply. Stream keys and the control token are replaced with placeholders.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/preset/save")) },
    MethodInfo { name: "preview.close", summary: "Give up a raw frame socket. The socket goes when the last holder closes it.", scope: "read", mutating: false, destructive: false, rest: Some(("POST", "/api/v1/preview/close")) },
    MethodInfo { name: "preview.open", summary: "Open a raw frame socket on this machine for a source or the programme, and answer with its path. No encode anywhere: a client on the same host reads the frames the mixer already has. Close it with preview.close.", scope: "read", mutating: false, destructive: false, rest: Some(("POST", "/api/v1/preview/open")) },
    MethodInfo { name: "program.get", summary: "What is on air, the programme running time, and what revert would go back to.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/program")) },
    MethodInfo { name: "program.golive", summary: "One call to put a web page on air: add the page, add the destination, and take the page as soon as it renders.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/program/golive")) },
    MethodInfo { name: "program.history", summary: "The last hundred takes, newest first, with the token that asked for each.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/program/history")) },
    MethodInfo { name: "program.revert", summary: "Take back to the shot before this one.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/program/revert")) },
    MethodInfo { name: "program.take", summary: "Put a scene or a source on programme. The cut is instant and the outgoing stream is not disturbed.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/program/take")) },
    MethodInfo { name: "program.thumbnail", summary: "What is on air as a small JPEG in base64, {jpeg, width, height, at_ms}, or {pending: true} while the first picture is on its way. An ask keeps one picture a second coming for ten seconds; nothing runs between asks.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/program/thumbnail")) },
    MethodInfo { name: "program.transitions", summary: "Every transition a take may name on this core, with the params each reads, and what an item's enter and exit may be.", scope: "read", mutating: false, destructive: false, rest: Some(("POST", "/api/v1/program/transitions")) },
    MethodInfo { name: "project.export", summary: "This mixer as one project file: settings, sources, outputs and renditions, channels, scenes, the page's layout, and its clips by name and size. Keys only with include_secrets.", scope: "admin", mutating: false, destructive: false, rest: Some(("POST", "/api/v1/project/export")) },
    MethodInfo { name: "project.import", summary: "Open a project file: answers with what it would change (dry_run is true unless false is sent), then replaces this mixer's setup or merges beside it. Says which settings wait for a restart.", scope: "admin", mutating: true, destructive: true, rest: Some(("POST", "/api/v1/project/import")) },
    MethodInfo { name: "rendition.plan", summary: "What the planner built for every output that asked for a rendition: each node, what it serves, which encoder and why, and the totals.", scope: "read", mutating: false, destructive: false, rest: Some(("POST", "/api/v1/rendition/plan")) },
    MethodInfo { name: "rendition.presets", summary: "Every rendition preset, priced on this machine by the governor. One this machine cannot make says so, with why.", scope: "read", mutating: false, destructive: false, rest: Some(("POST", "/api/v1/rendition/presets")) },
    MethodInfo { name: "scene.add", summary: "Make an empty scene, or one built from a set of sources.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes")) },
    MethodInfo { name: "scene.apply_graphic", summary: "Fill a graphic that is on a scene, by field name, and optionally play it on or take it off. Answers with the records and, if asked, a still.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/apply_graphic")) },
    MethodInfo { name: "scene.apply_layout", summary: "Apply a layout, making a scene or reshaping one that exists. Applying onto an existing scene keeps the item ids, so the change is a ramp and not a cut.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/apply_layout")) },
    MethodInfo { name: "scene.create_from", summary: "A scene from a set of sources, laid out by the built in layout for that count (full, two-box, three-box, quad, then a grid) or by a named one.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/create_from")) },
    MethodInfo { name: "scene.duplicate", summary: "A copy of a scene with new ids throughout, so editing the copy cannot touch the original.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/{id}/duplicate")) },
    MethodInfo { name: "scene.edit.apply", summary: "Write a draft back into the live document.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/edit/apply")) },
    MethodInfo { name: "scene.edit.begin", summary: "Take a working copy of a scene. Editing is off air by default: the draft is written back on the next take of that scene, or when you apply it.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/edit/begin")) },
    MethodInfo { name: "scene.edit.discard", summary: "Throw a draft away. The live document is untouched.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/edit/discard")) },
    MethodInfo { name: "scene.export", summary: "The whole collection: as JSON, or as a zip bundle carrying its assets with a hash each, which is what you send somebody.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/scenes/export")) },
    MethodInfo { name: "scene.get", summary: "One scene: its records and where every item actually lands on the canvas.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/scenes/{id}")) },
    MethodInfo { name: "scene.graphic.list", summary: "Every graphic template this core can place, with what each one takes.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/scenes/graphic/list")) },
    MethodInfo { name: "scene.history.mark", summary: "Group the changes that follow into one undo step, until the next mark. This is what makes a drag of forty moves one Ctrl+Z.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/history/mark")) },
    MethodInfo { name: "scene.import", summary: "Read a collection bundle, a zip or the directory it unpacks to, and add its scenes to this one. Answers with a relink report for any asset that did not come across.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/import")) },
    MethodInfo { name: "scene.import.obs", summary: "Read an OBS Studio scene collection and add its scenes to this one. Send the file's text as `content` (what a page's file picker reads) or a `path` on the mixer's machine. With `add_sources: true` the sources the scenes draw are added through source.add, and the answer says which were added and why any were not.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/import/obs")) },
    MethodInfo { name: "scene.item.add", summary: "Put something on a scene's canvas. With no transform it lands in the next free cell, so a drop never needs a dialog.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/item/add")) },
    MethodInfo { name: "scene.item.align", summary: "Line items up on an edge: left, right, top, bottom, center-x or center-y.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/item/align")) },
    MethodInfo { name: "scene.item.arrange_grid", summary: "Lay items out in a grid of `cols` columns.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/item/arrange_grid")) },
    MethodInfo { name: "scene.item.bind", summary: "Bind a geometry property to an expression over the collection's parameters, so changing a number moves everything that follows it.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/item/bind")) },
    MethodInfo { name: "scene.item.copy", summary: "Copy an item into another scene. The copy keeps the transform and the filters and gets a new id.", scope: "operate", mutating: true, destructive: false, rest: Some(("GET", "/api/v1/scenes/item/copy")) },
    MethodInfo { name: "scene.item.cover_canvas", summary: "Put items over the whole canvas, filling it and letting the overflow go.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/item/cover_canvas")) },
    MethodInfo { name: "scene.item.distribute", summary: "Space items evenly between the two on the ends, horizontally or vertically.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/item/distribute")) },
    MethodInfo { name: "scene.item.filter.add", summary: "Hang a filter on one item, so a camera keyed in one scene is not keyed in all of them.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/item/filter/add")) },
    MethodInfo { name: "scene.item.filter.remove", summary: "Take a filter off an item.", scope: "operate", mutating: true, destructive: true, rest: Some(("POST", "/api/v1/scenes/item/filter/remove")) },
    MethodInfo { name: "scene.item.filter.set", summary: "Change one of an item's filters, or turn it off without taking it out.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/item/filter/set")) },
    MethodInfo { name: "scene.item.fit_to_canvas", summary: "Put items over the whole canvas, keeping their aspect ratio inside it.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/item/fit_to_canvas")) },
    MethodInfo { name: "scene.item.group", summary: "Put items into a group. The picture does not change.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/item/group")) },
    MethodInfo { name: "scene.item.match_size", summary: "Make items the same size as another one.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/item/match_size")) },
    MethodInfo { name: "scene.item.move", summary: "Move an item to another scene, keeping its transform and filters.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/item/move")) },
    MethodInfo { name: "scene.item.remove", summary: "Take an item off a scene.", scope: "operate", mutating: true, destructive: true, rest: Some(("POST", "/api/v1/scenes/item/remove")) },
    MethodInfo { name: "scene.item.reorder", summary: "Move an item up or down the stack, between two named neighbours.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/item/reorder")) },
    MethodInfo { name: "scene.item.schema", summary: "What one item type takes: a graphic's OGraf schema, or a source or filter plugin's settings schema. The same JSON Schema every client renders an inspector from.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/scenes/item/schema")) },
    MethodInfo { name: "scene.item.set", summary: "Assign an item's properties. Only the keys named move; the rest are left alone, so calling it twice with the same body changes nothing the second time.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/item/set")) },
    MethodInfo { name: "scene.item.ungroup", summary: "Take a group apart, leaving every child exactly where it looked.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/item/ungroup")) },
    MethodInfo { name: "scene.layout.copy", summary: "Read one scene's geometry, to paste onto another.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/scenes/layout/copy")) },
    MethodInfo { name: "scene.layout.list", summary: "The layouts that ship with the core, with the parameters each one takes.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/scenes/layout/list")) },
    MethodInfo { name: "scene.layout.paste", summary: "Put one scene's geometry onto another's items, matched by name first and slot order second. Items that match nothing are left alone.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/layout/paste")) },
    MethodInfo { name: "scene.list", summary: "Every scene in the collection, with how many items it has, the sources it draws and whether it is armed.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/scenes")) },
    MethodInfo { name: "scene.params.get", summary: "The collection's typed parameters, readable without their values, so a client discovers what is fillable before filling it.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/scenes/params/get")) },
    MethodInfo { name: "scene.params.set", summary: "Set the collection's parameter values, declaring any that are new. A `{{name}}` in any string property of any item follows them, so one call changes every lower third that uses it.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/params/set")) },
    MethodInfo { name: "scene.preview.frame", summary: "A still of the armed scene as base64 JPEG, the floor every client has.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/scenes/preview/frame")) },
    MethodInfo { name: "scene.preview.set", summary: "Arm a scene. The armed scene is the preview, and program.take with no argument takes it.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/preview/set")) },
    MethodInfo { name: "scene.redo", summary: "Put back what undo took away.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/redo")) },
    MethodInfo { name: "scene.remove", summary: "Delete a scene. What is on air is not touched.", scope: "operate", mutating: true, destructive: true, rest: Some(("DELETE", "/api/v1/scenes/{id}")) },
    MethodInfo { name: "scene.rename", summary: "Change a scene's name, its colour, or both. Names and colours live on the document, so every client, the tally and an agent see the same ones.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/{id}/rename")) },
    MethodInfo { name: "scene.transaction.abort", summary: "Throw the batch away. The document goes back to where it was when the batch opened.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/transaction/abort")) },
    MethodInfo { name: "scene.transaction.begin", summary: "Start a batch. Everything until the commit applies on one frame or not at all, and undoes in one step.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/transaction/begin")) },
    MethodInfo { name: "scene.transaction.commit", summary: "Apply the batch.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/transaction/commit")) },
    MethodInfo { name: "scene.undo", summary: "Undo the last change. A drag marked with scene.history.mark undoes as one step.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/undo")) },
    MethodInfo { name: "scene.validate", summary: "Overlaps, items off the canvas, safe area breaches and missing sources: what to fix before saying a scene is done.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/scenes/validate")) },
    MethodInfo { name: "scene.virtual_set", summary: "A new scene with a presenter keyed in front of a background, and optionally a foreground such as a desk and a lower third area. Pictures from the media library become sources; the key colour is guessed from the camera.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/scenes/{id}/virtual_set")) },
    MethodInfo { name: "show.add", summary: "Make another show and start it: empty, a copy of a show (without its outputs, so nothing goes out twice), or from a project file.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/shows")) },
    MethodInfo { name: "show.add_many", summary: "Make many shows in one call, such as every channel of a headend. The whole batch is checked first. With dry_run (the default) nothing is made: the answer says what would be, what its renditions would cost and whether the governor would admit them. Without it, every show that fits is made and the rest are refused with why; a show is made whole or not at all.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/shows/add_many")) },
    MethodInfo { name: "show.list", summary: "Every show on this machine: its name, whether it is running, what is on air, what its outputs send and what its process costs. `current` is the show a client reaches when it names none.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/shows")) },
    MethodInfo { name: "show.output.add", summary: "Send a show without compositing to another place: an address (SRT, RTMP, UDP, RTP or RIST), a platform and its key, or hls://<name> to serve it as HLS from this port. Left without a rendition it copies the input's bytes; with one it is planned and admitted by the governor. The key is write only.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/shows/{id}/output/add")) },
    MethodInfo { name: "show.output.remove", summary: "Stop one output of a show without compositing and forget it, key and all.", scope: "admin", mutating: true, destructive: true, rest: Some(("POST", "/api/v1/shows/{id}/output/remove")) },
    MethodInfo { name: "show.output.set", summary: "Change one output of a show without compositing, naming only what moves: another address, a new key, on or off, copy or a rendition.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/shows/{id}/output")) },
    MethodInfo { name: "show.remove", summary: "Stop a show and remove it with its folder. Refused for the last show and for main, the show the station was started with.", scope: "admin", mutating: true, destructive: true, rest: Some(("DELETE", "/api/v1/shows/{id}")) },
    MethodInfo { name: "show.remove_many", summary: "Stop and remove many shows. Each id that cannot go (main, or one not there) is refused with why, and the rest go.", scope: "admin", mutating: true, destructive: true, rest: Some(("POST", "/api/v1/shows/remove_many")) },
    MethodInfo { name: "show.rename", summary: "Give a show another name. Its id stays.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/shows/{id}/rename")) },
    MethodInfo { name: "show.set", summary: "Change a show's name, its input, or whether it composites. Turning compositing on starts a show process whose one source is the input and moves the outputs to it; turning it off hands them back to the direct host, when the show has one source and no scenes in use. A switch can take half a minute, so it answers at once with a task_id and the show as it is; task.get with that id carries this answer, with how long the outputs were off, once it is done.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/shows/{id}/set")) },
    MethodInfo { name: "show.start", summary: "Start a stopped or failed show.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/shows/{id}/start")) },
    MethodInfo { name: "show.stats", summary: "Health, input numbers and each output's numbers for many shows in one read, from what the station already holds, so it is cheap to call every second for two hundred shows. `fields` narrows it to health, input or outputs.", scope: "read", mutating: false, destructive: false, rest: Some(("POST", "/api/v1/shows/stats")) },
    MethodInfo { name: "show.stop", summary: "Stop a show. It keeps its config, and stays stopped when the station starts again, until show.start.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/shows/{id}/stop")) },
    MethodInfo { name: "snapshot.get", summary: "One JPEG: the whole contact sheet, the programme, or one source cut out of the mosaic.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/snapshot/{id}")) },
    MethodInfo { name: "source.add", summary: "Add a source while the mixer runs. Answers with the id it got and the whole source record.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/sources")) },
    MethodInfo { name: "source.audio.set", summary: "Move a source's audio: the fader, the mute, and for a superimposed page the balance between its own sound and the videos under it.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/sources/{id}/audio")) },
    MethodInfo { name: "source.duplicate", summary: "Add another source like one the mixer has: the same address and settings under a new id. A client cannot do this with source.add, because the address it is shown has everything after the host cut off.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/sources/{id}/duplicate")) },
    MethodInfo { name: "source.get", summary: "One source. Refused with the ids that exist when there is no such source.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/sources/{id}")) },
    MethodInfo { name: "source.group", summary: "Put sources in a tray folder. A tag for finding things, not a group on the canvas.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/sources/{id}/group")) },
    MethodInfo { name: "source.key_color", summary: "The colour to key a source on: the colour at a point of its picture, or with no point the green or blue screen it stands in front of.", scope: "read", mutating: false, destructive: false, rest: Some(("POST", "/api/v1/sources/{id}/key_color")) },
    MethodInfo { name: "source.list", summary: "Every source, with its state, whether it has video and audio, and its fader.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/sources")) },
    MethodInfo { name: "source.missing", summary: "Sources that are not running, and why: failed, could not be started (with the error and the action that fixes it), removed, or unknown. Pass the ids a scene draws, or none for every one the mixer knows about.", scope: "read", mutating: false, destructive: false, rest: Some(("POST", "/api/v1/sources/{id}/missing")) },
    MethodInfo { name: "source.remove", summary: "Remove a source. If it is on programme the mixer cuts to the slate first.", scope: "operate", mutating: true, destructive: true, rest: Some(("DELETE", "/api/v1/sources/{id}")) },
    MethodInfo { name: "source.restart", summary: "Build a source's pipeline again now, rather than waiting for its next retry. For a source that could not be started or was removed, use source.restore.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/sources/{id}/restart")) },
    MethodInfo { name: "source.restore", summary: "Put back a source that source.remove took away, as it was: same id, address, settings, fader and mute. The mixer remembers the last sixteen it removed, until it restarts.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/sources/{id}/restore")) },
    MethodInfo { name: "source.seek", summary: "Move a seekable source to a position. Answers with where it actually landed.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/sources/{id}/seek")) },
    MethodInfo { name: "source.set", summary: "Change a running source: its name and colour, its params, or where it runs. The name and colour live on the scene document. Moving a source between the core, a sidecar and a node is `place`; the programme keeps its frame rate across the move and the compositor covers the swap.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/sources/{id}/set")) },
    MethodInfo { name: "task.cancel", summary: "Ask a piece of long running work to stop. Cooperative: the answer says the request landed, not that the work has stopped yet.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/tasks/{id}/cancel")) },
    MethodInfo { name: "task.get", summary: "How a piece of long running work is getting on, and its answer once it has one.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/tasks/{id}")) },
    MethodInfo { name: "task.list", summary: "Every background job this core knows about, newest first.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/tasks")) },
    MethodInfo { name: "tool.call", summary: "Call one of a plugin's tools, in MCP's shape. The name is `<plugin>/<tool>`, or the bare tool name when only one plugin has it.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/tool/call")) },
    MethodInfo { name: "vitals.get", summary: "This show's health (its state and alarms, null in the first second) and the thresholds they are judged by.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/vitals")) },
    MethodInfo { name: "vitals.set", summary: "Change the alarm thresholds, or whether a mosaic is kept up for the black and freeze checks while nobody is looking. Fields left out keep their defaults; a duration of 0 switches that check off. Applies within a second.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/vitals/set")) },
];

pub const EVENT_NAMES: [&str; 30] = [
    "snapshot",
    "program.took",
    "scene.patch",
    "preview.changed",
    "source.state",
    "source.position",
    "output.state",
    "adbreak.changed",
    "ui.changed",
    "hook.blocked",
    "media.changed",
    "channel.changed",
    "channel.removed",
    "channel.refused",
    "meters",
    "tally",
    "alert",
    "telemetry",
    "agent.state",
    "multiview.layout",
    "multiview.frame",
    "preview.frame",
    "resync",
    "flush",
    "rendition.plan",
    "governor.shed",
    "show.changed",
    "show.removed",
    "show.health",
    "health",
];

pub const EXT_KEYS: [&str; 8] = [
    "multiview",
    "meters",
    "tally",
    "positions",
    "thumb",
    "preview",
    "telemetry",
    "agent",
];

/// One event off the wire, already parsed into its payload.
///
/// `Other` is not a failure: a core a level ahead sends events this build has
/// never heard of, and a client that panicked on one would be useless.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The full state, and the sequence number it is current as of. Sent on subscribe and after any change the deltas cannot describe.
    Snapshot(Snapshot),
    /// The programme changed. Carries the running time the cut landed on, so a client can see how close a scheduled take was to its mark.
    ProgramTook(ProgramTookEvent),
    /// One change to the scene document, as records rather than a snapshot: what was added, what changed with its before and after, and what was removed. One per transaction, batched and ended by event/flush.
    ScenePatch(ScenePatchEvent),
    /// A scene was armed, or the arming was cleared. The armed scene is the preview, and program.take with no argument takes it.
    PreviewChanged(PreviewChangedEvent),
    /// A source moved between connecting, live, stalled and failed.
    SourceState(SourceStateEvent),
    /// How far through a seekable source has got, a few times a second. Never sent for a camera, which has no position to report.
    SourcePosition(SourcePositionEvent),
    /// A destination connected, dropped or is retrying.
    OutputState(OutputStateEvent),
    /// An ad break was armed, went on air, or ended.
    AdbreakChanged(AdbreakChangedEvent),
    /// The surface defaults changed: a preset was applied, or an operator set the layout, theme or gallery mode by hand. Nothing on air moves.
    UiChanged(UiChangedEvent),
    /// A hook did not get its say: it did not answer inside its timeout, or the thing behind it could not be reached. Whatever the hook was attached to went ahead anyway, which is the rule that keeps a slow hook off the frame path. See 03 section 8.
    HookBlocked(HookBlockedEvent),
    /// A file in the library was uploaded, deleted, or its conversion moved on.
    MediaChanged(MediaChangedEvent),
    /// A channel changed: made, renamed, switched on or off, a key made or taken back, a stream went live, learned its codecs or left. Carries the whole channel, keys as hints only.
    ChannelChanged(ChannelChangedEvent),
    /// A channel was removed. Its publishers were turned away and its keys forgotten.
    ChannelRemoved(ChannelRemovedEvent),
    /// A publisher was turned away: no key, a wrong key, a channel switched off, or a name somebody else is already publishing. why is the sentence the encoder was sent. A key is never in it.
    ChannelRefused(ChannelRefusedEvent),
    /// Peak dBFS for the programme bus and every source, in one message at 10 per second. Replaces the two separate meter events on /ws.
    Meters(Meters),
    /// Which sources are on programme, on preview, or off. Derived by the core so a Stream Deck does not have to.
    Tally(Tally),
    /// Something an operator should see. Also written to the log and to the alert webhook.
    Alert(AlertEvent),
    /// Numbers instead of a picture, up to ten times a second and under 200 bytes: the shot change score, the black ratio, a freeze flag, short term and integrated loudness, a silence flag and which sources are live. From cheap probes on the raw programme frames, which run only while a client is subscribed.
    Telemetry(TelemetryEvent),
    /// The agent.state document, pushed when a telemetry threshold crosses or a take lands, with `why` naming which and a snapshot URL beside it. Edge triggered and at most one a second, so a picture that stays black is one message rather than one a tick.
    AgentState(BTreeMap<String, Value>),
    /// How to read the binary frames that follow: the cells, and the layout id carried in every frame header.
    MultiviewLayout(MultiviewLayout),
    /// 16 byte header then JPEG, decoded by [`crate::frames::parse_frame`].
    MultiviewFrame(crate::frames::Frame),
    /// 16 byte header then JPEG, decoded by [`crate::frames::parse_frame`].
    PreviewFrame(crate::frames::Frame),
    /// This client fell behind and events were dropped. Re-subscribe for a fresh snapshot; nothing between from_seq and the new snapshot arrives.
    Resync(Resync),
    /// The end of a batch. Render here and not before, so a client never paints half an update.
    Flush(Flush),
    /// The programme's rendition plan changed: an output that asks for a rendition was added, changed or removed, or the governor stopped or brought back an encoder. plan is what rendition.plan answers.
    RenditionPlan(RenditionPlanEvent),
    /// The machine ran short while on air and the governor stopped something to keep what is on air whole: what it was and why. It is brought back by itself when there is room again.
    GovernorShed(ShedNote),
    /// A show was added, renamed, started, stopped, died or came back. Sent by the station to every client, whichever show it is looking at.
    ShowChanged(ShowChanged),
    /// A show was removed. Its process was stopped first.
    ShowRemoved(ShowRemovedEvent),
    /// A show's health changed state, or an alarm began or ended. Never sent for a number alone: read those with show.stats.
    ShowHealth(ShowHealthEvent),
    /// This show's health changed: its state (ok, warning, alarm, off) or the kinds of its alarms, never a number alone. From a show that composites; the station sends it on to every client as show.health with the show's id. docs/reference/show-health.md says what each alarm watches.
    Health(HealthEvent),
    /// An event name this api_level does not know, with its params as they came.
    Other { name: String, params: Value },
}

impl Event {
    /// Parse one `event/...` notification. Unknown names and payloads that do
    /// not fit become [`Event::Other`] rather than an error.
    pub fn parse(name: &str, params: Value) -> Event {
        let pattern = name.strip_prefix("event/").unwrap_or(name);
        match pattern {
            "snapshot" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::Snapshot(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "program.took" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::ProgramTook(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "scene.patch" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::ScenePatch(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "preview.changed" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::PreviewChanged(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "source.state" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::SourceState(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "source.position" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::SourcePosition(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "output.state" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::OutputState(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "adbreak.changed" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::AdbreakChanged(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "ui.changed" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::UiChanged(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "hook.blocked" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::HookBlocked(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "media.changed" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::MediaChanged(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "channel.changed" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::ChannelChanged(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "channel.removed" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::ChannelRemoved(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "channel.refused" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::ChannelRefused(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "meters" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::Meters(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "tally" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::Tally(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "alert" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::Alert(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "telemetry" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::Telemetry(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "agent.state" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::AgentState(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "multiview.layout" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::MultiviewLayout(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "resync" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::Resync(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "flush" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::Flush(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "rendition.plan" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::RenditionPlan(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "governor.shed" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::GovernorShed(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "show.changed" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::ShowChanged(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "show.removed" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::ShowRemoved(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "show.health" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::ShowHealth(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            "health" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::Health(payload),
                Err(_) => Event::Other { name: pattern.to_string(), params },
            },
            _ => Event::Other { name: pattern.to_string(), params },
        }
    }

    /// The name after `event/`, whatever the variant.
    pub fn name(&self) -> &str {
        match self {
            Event::Snapshot(_) => "snapshot",
            Event::ProgramTook(_) => "program.took",
            Event::ScenePatch(_) => "scene.patch",
            Event::PreviewChanged(_) => "preview.changed",
            Event::SourceState(_) => "source.state",
            Event::SourcePosition(_) => "source.position",
            Event::OutputState(_) => "output.state",
            Event::AdbreakChanged(_) => "adbreak.changed",
            Event::UiChanged(_) => "ui.changed",
            Event::HookBlocked(_) => "hook.blocked",
            Event::MediaChanged(_) => "media.changed",
            Event::ChannelChanged(_) => "channel.changed",
            Event::ChannelRemoved(_) => "channel.removed",
            Event::ChannelRefused(_) => "channel.refused",
            Event::Meters(_) => "meters",
            Event::Tally(_) => "tally",
            Event::Alert(_) => "alert",
            Event::Telemetry(_) => "telemetry",
            Event::AgentState(_) => "agent.state",
            Event::MultiviewLayout(_) => "multiview.layout",
            Event::MultiviewFrame(_) => "multiview.frame",
            Event::PreviewFrame(_) => "preview.frame",
            Event::Resync(_) => "resync",
            Event::Flush(_) => "flush",
            Event::RenditionPlan(_) => "rendition.plan",
            Event::GovernorShed(_) => "governor.shed",
            Event::ShowChanged(_) => "show.changed",
            Event::ShowRemoved(_) => "show.removed",
            Event::ShowHealth(_) => "show.health",
            Event::Health(_) => "health",
            Event::Other { name, .. } => name,
        }
    }
}

/// One method per protocol method. Thin wrappers over [`Client::call`], so a
/// method the core gains is one regenerated line here.
impl Client {
    /// Cut a running ad short, or disarm one that is scheduled.
    pub async fn adbreak_end(&self) -> Result<BTreeMap<String, Value>> {
        self.call("adbreak.end", &serde_json::json!({})).await
    }

    /// Interrupt the programme with a clip, then rejoin live when it ends.
    pub async fn adbreak_start(&self, params: &AdBreakRequest) -> Result<BTreeMap<String, Value>> {
        self.call("adbreak.start", params).await
    }

    /// The compact document written for agents: the programme, each source's state and a motion score saying how much its picture is changing.
    pub async fn agent_state(&self, params: &AgentStateRequest) -> Result<BTreeMap<String, Value>> {
        self.call("agent.state", params).await
    }

    /// Make a channel and its first key, which is in this answer. channel.key.reveal reads it again later.
    pub async fn channel_add(&self, params: &ChannelAddRequest) -> Result<ChannelAdded> {
        self.call("channel.add", params).await
    }

    /// Make a self signed certificate for RTMPS, for this machine's addresses unless names are given. Encoders must be told to accept it; one from a certificate authority needs no such step.
    pub async fn channel_certificate_generate(&self, params: &CertificateGenerateRequest) -> Result<CertificateInfo> {
        self.call("channel.certificate.generate", params).await
    }

    /// Give RTMPS a certificate: the PEM of the certificate (and its chain) and of its private key, as a certificate authority issued them. Checked before it is kept; the key is sealed and never read back.
    pub async fn channel_certificate_set(&self, params: &CertificateSetRequest) -> Result<CertificateInfo> {
        self.call("channel.certificate.set", params).await
    }

    /// Send a channel's stream on to YouTube, Facebook, Twitch, an RTMP server or an SRT receiver as it arrives. Nothing is decoded or encoded. The key is write only.
    pub async fn channel_destination_add(&self, params: &AddDestinationRequest) -> Result<BTreeMap<String, Value>> {
        self.call("channel.destination.add", params).await
    }

    /// Stop sending a channel's stream to one destination and forget it. The publisher and the other destinations are not touched.
    pub async fn channel_destination_remove(&self, params: &RemoveDestinationRequest) -> Result<BTreeMap<String, Value>> {
        self.call("channel.destination.remove", params).await
    }

    /// Change one of a channel's destinations, naming only what moves: a new key, another server, which stream it sends, on or off. A key left out is kept.
    pub async fn channel_destination_set(&self, params: &SetDestinationRequest) -> Result<BTreeMap<String, Value>> {
        self.call("channel.destination.set", params).await
    }

    /// One channel.
    pub async fn channel_get(&self, params: &IdRequest) -> Result<Channel> {
        self.call("channel.get", params).await
    }

    /// Make another key for a channel, to give to one more person or encoder. The key is in this answer, and channel.key.reveal reads it again later.
    pub async fn channel_key_add(&self, params: &ChannelKeyAddRequest) -> Result<KeyAdded> {
        self.call("channel.key.add", params).await
    }

    /// Take one key back. A publisher on air with it is cut off and the next one is turned away; the other keys are untouched.
    pub async fn channel_key_remove(&self, params: &ChannelKeyRemoveRequest) -> Result<Channel> {
        self.call("channel.key.remove", params).await
    }

    /// Read one key of a channel back, to give it to an encoder again. Admin only; a list shows only the last four characters. Each read is logged with who asked, never with the key.
    pub async fn channel_key_reveal(&self, params: &ChannelKeyRevealRequest) -> Result<KeyRevealed> {
        self.call("channel.key.reveal", params).await
    }

    /// Every channel with its keys (as hints), the address to publish to over each protocol it has on, and what is live on it; and which ingest ports are open and for which channels.
    pub async fn channel_list(&self) -> Result<ChannelList> {
        self.call("channel.list", &serde_json::json!({})).await
    }

    /// Remove a channel and forget its keys. Sources it made that no scene holds go with it.
    pub async fn channel_remove(&self, params: &IdRequest) -> Result<ChannelRemoved> {
        self.call("channel.remove", params).await
    }

    /// Rename a channel, switch it on or off, or change its application name, whether its streams become sources, how its key is given, which protocols it takes (rtmp, srt, whip) or RTMPS and its port. A port opens when the first channel needs it and closes when the last one stops. Only what is named moves.
    pub async fn channel_set(&self, params: &ChannelSetRequest) -> Result<Channel> {
        self.call("channel.set", params).await
    }

    /// Every codec and element in the catalogue, which of them this machine actually has, and what it would pick.
    pub async fn codec_list(&self) -> Result<BTreeMap<String, Value>> {
        self.call("codec.list", &serde_json::json!({})).await
    }

    /// The mixer's settings: each key's value in the config file, its default, when a change to it takes effect, and which keys are waiting for a restart. Secrets say only whether one is set.
    pub async fn config_get(&self, params: &ConfigGetRequest) -> Result<ConfigGetResult> {
        self.call("config.get", params).await
    }

    /// Put settings back to their defaults by taking them out of the config file. Answers like config.set.
    pub async fn config_reset(&self, params: &ConfigResetRequest) -> Result<ConfigSetResult> {
        self.call("config.reset", params).await
    }

    /// Every setting config.set takes, as one JSON Schema: type, title, description, default, range or choices, and x-gmx-applies (live, next_source or restart).
    pub async fn config_schema(&self) -> Result<BTreeMap<String, Value>> {
        self.call("config.schema", &serde_json::json!({})).await
    }

    /// Change settings in the config file, keeping its comments. Every value is checked first and nothing is written unless all of them fit. Live keys take effect at once; the answer says which wait for the next source or a restart.
    pub async fn config_set(&self, params: &ConfigSetRequest) -> Result<ConfigSetResult> {
        self.call("config.set", params).await
    }

    /// Every method, event and type as JSON Schema. The same document as protocol.json and `godwinmix --api-info`.
    pub async fn core_api(&self) -> Result<BTreeMap<String, Value>> {
        self.call("core.api", &serde_json::json!({})).await
    }

    /// The environment checks: GStreamer, the elements, the config, the disk and the ports. The same list `gmx doctor` prints.
    pub async fn core_doctor(&self) -> Result<BTreeMap<String, Value>> {
        self.call("core.doctor", &serde_json::json!({})).await
    }

    /// What this core is, what it can do, and where its edges are.
    pub async fn core_info(&self) -> Result<CoreInfo> {
        self.call("core.info", &serde_json::json!({})).await
    }

    /// Stop the mixer and have it started again, when something will start it again. On a supervised core (core.info restart.possible) it answers restarting: true and exits; the programme is off air until it is back. On a core started by hand it answers restarting: false, says how to restart it, and keeps running.
    pub async fn core_restart(&self) -> Result<RestartAnswer> {
        self.call("core.restart", &serde_json::json!({})).await
    }

    /// The append only record of everything that happened, back as far as you ask.
    pub async fn core_session_log(&self, params: &SessionLogRequest) -> Result<BTreeMap<String, Value>> {
        self.call("core.session_log", params).await
    }

    /// Stop the mixer, and with it the programme. Nothing else takes the show off air, so this is deliberately its own call.
    pub async fn core_shutdown(&self) -> Result<BTreeMap<String, Value>> {
        self.call("core.shutdown", &serde_json::json!({})).await
    }

    /// How long each stage of the start took, and what was over the 250 ms mark.
    pub async fn core_startup_report(&self) -> Result<BTreeMap<String, Value>> {
        self.call("core.startup_report", &serde_json::json!({})).await
    }

    /// The full state: programme, every source, every output, the multiview grid, the encoder backend and any ad break.
    pub async fn core_status(&self) -> Result<MixerStatus> {
        self.call("core.status", &serde_json::json!({})).await
    }

    /// Subscribe to the event stream. WebSocket only: the core answers event/snapshot then deltas, ending every batch with event/flush.
    pub async fn core_subscribe(&self, params: &SubscribeRequest) -> Result<SubscribeResult> {
        self.call("core.subscribe", params).await
    }

    /// Ask every device plugin what it can see: cameras, NDI senders, publishers. Each candidate's params are ready for source.add.
    pub async fn device_discover(&self, params: &DiscoverRequest) -> Result<BTreeMap<String, Value>> {
        self.call("device.discover", params).await
    }

    /// Hang a filter on one source or on the programme, live.
    pub async fn filter_add(&self, params: &AddFilterRequest) -> Result<FilterRecord> {
        self.call("filter.add", params).await
    }

    /// Every filter in place, with what it is and where it sits.
    pub async fn filter_list(&self) -> Result<FilterListing> {
        self.call("filter.list", &serde_json::json!({})).await
    }

    /// Take a filter out of the pipeline.
    pub async fn filter_remove(&self, params: &FilterIdRequest) -> Result<FilterRemoved> {
        self.call("filter.remove", params).await
    }

    /// Change a filter's settings in place. A filter that cannot take the change while running says so rather than being restarted behind your back.
    pub async fn filter_set(&self, params: &SetFilterRequest) -> Result<FilterRecord> {
        self.call("filter.set", params).await
    }

    /// Measure this machine's encoders again, in the background, a few seconds of every core. Refused while anything is on air unless `confirm` is true.
    pub async fn governor_calibrate(&self, params: &CalibrateRequest) -> Result<CalibrateResult> {
        self.call("governor.calibrate", params).await
    }

    /// The resource governor: when this machine was measured, what is in use and free on the CPU and each GPU encoder, and what was shed to keep the programme whole.
    pub async fn governor_status(&self) -> Result<GovernorStatus> {
        self.call("governor.status", &serde_json::json!({})).await
    }

    /// Raise GStreamer's own debug categories for a while, then let them fall back on their own.
    pub async fn log_gst(&self, params: &LogGstRequest) -> Result<LogGstResult> {
        self.call("log.gst", params).await
    }

    /// Every log level override in force, and the GStreamer categories still raised.
    pub async fn log_levels(&self) -> Result<BTreeMap<String, Value>> {
        self.call("log.levels", &serde_json::json!({})).await
    }

    /// Change one instance's or one module's log level while the mixer runs.
    pub async fn log_set(&self, params: &LogSetRequest) -> Result<BTreeMap<String, Value>> {
        self.call("log.set", params).await
    }

    /// Transcode a library file to a web safe copy, in the background.
    pub async fn media_convert(&self, params: &NameRequest) -> Result<ConversionState> {
        self.call("media.convert", params).await
    }

    /// The clips in the library, with durations and whether each has audio.
    pub async fn media_list(&self) -> Result<MediaListing> {
        self.call("media.list", &serde_json::json!({})).await
    }

    /// Delete a library file and its converted copy. Refused while it is a live source.
    pub async fn media_remove(&self, params: &NameRequest) -> Result<BTreeMap<String, Value>> {
        self.call("media.remove", params).await
    }

    /// Stream a file into the library. HTTP only: the body is the file.
    pub async fn media_upload(&self) -> Result<BTreeMap<String, Value>> {
        self.call("media.upload", &serde_json::json!({})).await
    }

    /// Look for nodes on the local network over mDNS. A network without multicast finds nothing and the [nodes] table in the config is the way there.
    pub async fn node_discover(&self, params: &DiscoverRequest2) -> Result<DiscoverAnswer> {
        self.call("node.discover", params).await
    }

    /// Mint a one time enrolment token for a node. The answer carries the command to run on the other machine. The token is good for one enrolment and expires.
    pub async fn node_enrol(&self, params: &EnrolRequest) -> Result<BTreeMap<String, Value>> {
        self.call("node.enrol", params).await
    }

    /// One node: its clock offset, how long since its last heartbeat, the plugins it has, and the instances it is hosting.
    pub async fn node_get(&self, params: &NodeName) -> Result<NodeView> {
        self.call("node.get", params).await
    }

    /// Every node this core knows about: the ones connected now, the ones that have gone quiet, and the ones the config expects that have never dialled in.
    pub async fn node_list(&self) -> Result<NodeListing> {
        self.call("node.list", &serde_json::json!({})).await
    }

    /// Forget a node. Its bridge is closed, every token minted for a plugin on it is revoked, and its certificate stops working. Sources placed on it go to the slate until they are moved or the node enrols again.
    pub async fn node_remove(&self, params: &NodeName) -> Result<BTreeMap<String, Value>> {
        self.call("node.remove", params).await
    }

    /// Send the programme to another destination. The encoder is shared, so adding one costs nothing on air.
    pub async fn output_add(&self, params: &AddOutputRequest) -> Result<OutputStatus> {
        self.call("output.add", params).await
    }

    /// One destination.
    pub async fn output_get(&self, params: &IdRequest) -> Result<OutputStatus> {
        self.call("output.get", params).await
    }

    /// Every destination, with its state, reconnect count and how much is buffered.
    pub async fn output_list(&self) -> Result<Vec<OutputStatus>> {
        self.call("output.list", &serde_json::json!({})).await
    }

    /// Drop and re-establish one destination's connection now, without waiting for its reconnect policy.
    pub async fn output_reconnect(&self, params: &IdRequest) -> Result<OutputStatus> {
        self.call("output.reconnect", params).await
    }

    /// Stop sending to a destination and forget it. Other outputs are unaffected.
    pub async fn output_remove(&self, params: &IdRequest) -> Result<BTreeMap<String, Value>> {
        self.call("output.remove", params).await
    }

    /// Change a destination in place: a new address with a new stream key, a new reconnect policy, a deeper outage buffer. The address is write only, so a client that only wants the buffer never has to hold the key.
    pub async fn output_set(&self, params: &SetOutputRequest) -> Result<OutputStatus> {
        self.call("output.set", params).await
    }

    /// Make one new folder inside a folder path.list shows, and list it. A folder that is already there is listed rather than refused.
    pub async fn path_create(&self, params: &PathCreateRequest) -> Result<PathListing> {
        self.call("path.create", params).await
    }

    /// The folders in one folder on the mixer, and whether each is writable, for a folder picker. Only the home folder and the mixer's own folders are shown; files never are.
    pub async fn path_list(&self, params: &PathListRequest) -> Result<PathListing> {
        self.call("path.list", params).await
    }

    /// The clock every pipeline is running against, and how far each one has got.
    pub async fn pipeline_clock(&self) -> Result<BTreeMap<String, Value>> {
        self.call("pipeline.clock", &serde_json::json!({})).await
    }

    /// One pipeline as a graphviz graph: every element, every pad and the caps negotiated between them.
    pub async fn pipeline_dot(&self, params: &PipelineRequest) -> Result<PipelineDot> {
        self.call("pipeline.dot", params).await
    }

    /// How much delay one pipeline is carrying, and which stage put it there.
    pub async fn pipeline_latency(&self, params: &PipelineRequest) -> Result<BTreeMap<String, Value>> {
        self.call("pipeline.latency", params).await
    }

    /// Every pipeline running right now, by the name the other pipeline methods accept.
    pub async fn pipeline_list(&self) -> Result<BTreeMap<String, Value>> {
        self.call("pipeline.list", &serde_json::json!({})).await
    }

    /// Every queue in one pipeline with how full it is, fullest first. A queue that stays full is where the trouble is.
    pub async fn pipeline_queues(&self, params: &PipelineRequest) -> Result<BTreeMap<String, Value>> {
        self.call("pipeline.queues", params).await
    }

    /// Install a plugin, while live, from any source form: a GitHub release (owner/repo), a git URL, cargo:, npm:, pypi:, a local directory, or a bare name looked up in the marketplaces this mixer knows. The signature and the api level are checked before anything is copied.
    pub async fn plugin_add(&self, params: &AddPluginRequest) -> Result<PluginRecord> {
        self.call("plugin.add", params).await
    }

    /// One plugin in full: its manifest, the settings schema of every provide, and the description from each SKILL.md.
    pub async fn plugin_describe(&self, params: &PluginName) -> Result<PluginDescription> {
        self.call("plugin.describe", params).await
    }

    /// Turn a plugin off without uninstalling it. It registers nothing and runs no process until it is enabled again.
    pub async fn plugin_disable(&self, params: &PluginName) -> Result<PluginRecord> {
        self.call("plugin.disable", params).await
    }

    /// Turn a plugin back on. It registers what it declares and its instances start.
    pub async fn plugin_enable(&self, params: &PluginName) -> Result<PluginRecord> {
        self.call("plugin.enable", params).await
    }

    /// Every plugin installed, with what it provides and what each running instance is costing in cpu, memory, latency, dropped buffers and restarts.
    pub async fn plugin_list(&self) -> Result<PluginListing> {
        self.call("plugin.list", &serde_json::json!({})).await
    }

    /// Read a plugin's directory again and swap its running instances one at a time, with the freeze frame covering each.
    pub async fn plugin_reload(&self, params: &PluginName) -> Result<PluginRecord> {
        self.call("plugin.reload", params).await
    }

    /// Uninstall a plugin and unwind everything it registered: its provides, its tools, its panels, its hooks and its discovery matchers.
    pub async fn plugin_remove(&self, params: &PluginName) -> Result<PluginRemoved> {
        self.call("plugin.remove", params).await
    }

    /// Search every marketplace this mixer knows for a plugin, by name, description or kind. Answers what `gmx plugin add <name>` would install.
    pub async fn plugin_search(&self, params: &SearchRequest) -> Result<SearchResults> {
        self.call("plugin.search", params).await
    }

    /// A plugin's settings as they stand, with its schema beside them.
    pub async fn plugin_settings_get(&self, params: &PluginName) -> Result<PluginSettings> {
        self.call("plugin.settings.get", params).await
    }

    /// Change a plugin's settings. A plugin that cannot take a change while running says so rather than being restarted behind your back.
    pub async fn plugin_settings_set(&self, params: &SetSettingsRequest) -> Result<PluginSettings> {
        self.call("plugin.settings.set", params).await
    }

    /// Per instance cpu, memory, media latency, dropped buffers and restarts, refreshed once a second.
    pub async fn plugin_stats(&self) -> Result<StatsListing> {
        self.call("plugin.stats", &serde_json::json!({})).await
    }

    /// Fetch a newer build of a plugin, install it beside the one that is running, and prove it starts. A build that does not answer `initialize` within ten seconds is rolled back and the plugin that was working stays working.
    pub async fn plugin_update(&self, params: &UpdatePluginRequest) -> Result<PluginUpdated> {
        self.call("plugin.update", params).await
    }

    /// Put a preset on this core: its config, its scenes, its layout, its theme and its gallery mode. Pass dry_run to get the plan and write nothing.
    pub async fn preset_apply(&self, params: &ApplyRequest) -> Result<ApplyResult> {
        self.call("preset.apply", params).await
    }

    /// Every preset this core can apply: the six built in, plus anything installed beside the binary or under ~/.godwinmix/presets.
    pub async fn preset_list(&self) -> Result<BTreeMap<String, Value>> {
        self.call("preset.list", &serde_json::json!({})).await
    }

    /// Turn this core's working setup into a preset directory somebody else can apply. Stream keys and the control token are replaced with placeholders.
    pub async fn preset_save(&self, params: &SaveRequest) -> Result<BTreeMap<String, Value>> {
        self.call("preset.save", params).await
    }

    /// Give up a raw frame socket. The socket goes when the last holder closes it.
    pub async fn preview_close(&self, params: &PreviewOpenRequest) -> Result<PreviewClosed> {
        self.call("preview.close", params).await
    }

    /// Open a raw frame socket on this machine for a source or the programme, and answer with its path. No encode anywhere: a client on the same host reads the frames the mixer already has. Close it with preview.close.
    pub async fn preview_open(&self, params: &PreviewOpenRequest) -> Result<PreviewSocket> {
        self.call("preview.open", params).await
    }

    /// What is on air, the programme running time, and what revert would go back to.
    pub async fn program_get(&self) -> Result<ProgramState> {
        self.call("program.get", &serde_json::json!({})).await
    }

    /// One call to put a web page on air: add the page, add the destination, and take the page as soon as it renders.
    pub async fn program_golive(&self, params: &GoLiveRequest) -> Result<GoLiveResult> {
        self.call("program.golive", params).await
    }

    /// The last hundred takes, newest first, with the token that asked for each.
    pub async fn program_history(&self, params: &HistoryRequest) -> Result<Vec<TakeRecord>> {
        self.call("program.history", params).await
    }

    /// Take back to the shot before this one.
    pub async fn program_revert(&self) -> Result<ProgramState> {
        self.call("program.revert", &serde_json::json!({})).await
    }

    /// Put a scene or a source on programme. The cut is instant and the outgoing stream is not disturbed.
    pub async fn program_take(&self, params: &TakeRequest) -> Result<ProgramState> {
        self.call("program.take", params).await
    }

    /// What is on air as a small JPEG in base64, {jpeg, width, height, at_ms}, or {pending: true} while the first picture is on its way. An ask keeps one picture a second coming for ten seconds; nothing runs between asks.
    pub async fn program_thumbnail(&self, params: &ThumbnailRequest) -> Result<BTreeMap<String, Value>> {
        self.call("program.thumbnail", params).await
    }

    /// Every transition a take may name on this core, with the params each reads, and what an item's enter and exit may be.
    pub async fn program_transitions(&self) -> Result<TransitionCatalogue> {
        self.call("program.transitions", &serde_json::json!({})).await
    }

    /// This mixer as one project file: settings, sources, outputs and renditions, channels, scenes, the page's layout, and its clips by name and size. Keys only with include_secrets.
    pub async fn project_export(&self, params: &ExportRequest) -> Result<BTreeMap<String, Value>> {
        self.call("project.export", params).await
    }

    /// Open a project file: answers with what it would change (dry_run is true unless false is sent), then replaces this mixer's setup or merges beside it. Says which settings wait for a restart.
    pub async fn project_import(&self, params: &ImportRequest) -> Result<Report> {
        self.call("project.import", params).await
    }

    /// What the planner built for every output that asked for a rendition: each node, what it serves, which encoder and why, and the totals.
    pub async fn rendition_plan(&self, params: &PlanRequest) -> Result<PlanView> {
        self.call("rendition.plan", params).await
    }

    /// Every rendition preset, priced on this machine by the governor. One this machine cannot make says so, with why.
    pub async fn rendition_presets(&self) -> Result<PresetsResult> {
        self.call("rendition.presets", &serde_json::json!({})).await
    }

    /// Make an empty scene, or one built from a set of sources.
    pub async fn scene_add(&self, params: &AddSceneRequest) -> Result<SceneView> {
        self.call("scene.add", params).await
    }

    /// Fill a graphic that is on a scene, by field name, and optionally play it on or take it off. Answers with the records and, if asked, a still.
    pub async fn scene_apply_graphic(&self, params: &ApplyGraphicRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.apply_graphic", params).await
    }

    /// Apply a layout, making a scene or reshaping one that exists. Applying onto an existing scene keeps the item ids, so the change is a ramp and not a cut.
    pub async fn scene_apply_layout(&self, params: &ApplyLayoutRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.apply_layout", params).await
    }

    /// A scene from a set of sources, laid out by the built in layout for that count (full, two-box, three-box, quad, then a grid) or by a named one.
    pub async fn scene_create_from(&self, params: &CreateFromRequest) -> Result<SceneView> {
        self.call("scene.create_from", params).await
    }

    /// A copy of a scene with new ids throughout, so editing the copy cannot touch the original.
    pub async fn scene_duplicate(&self, params: &DuplicateSceneRequest) -> Result<SceneView> {
        self.call("scene.duplicate", params).await
    }

    /// Write a draft back into the live document.
    pub async fn scene_edit_apply(&self, params: &DraftRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.edit.apply", params).await
    }

    /// Take a working copy of a scene. Editing is off air by default: the draft is written back on the next take of that scene, or when you apply it.
    pub async fn scene_edit_begin(&self, params: &EditBeginRequest) -> Result<DraftRecord> {
        self.call("scene.edit.begin", params).await
    }

    /// Throw a draft away. The live document is untouched.
    pub async fn scene_edit_discard(&self, params: &DraftRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.edit.discard", params).await
    }

    /// The whole collection: as JSON, or as a zip bundle carrying its assets with a hash each, which is what you send somebody.
    pub async fn scene_export(&self, params: &ExportRequest2) -> Result<BTreeMap<String, Value>> {
        self.call("scene.export", params).await
    }

    /// One scene: its records and where every item actually lands on the canvas.
    pub async fn scene_get(&self, params: &SceneRequest) -> Result<SceneView> {
        self.call("scene.get", params).await
    }

    /// Every graphic template this core can place, with what each one takes.
    pub async fn scene_graphic_list(&self) -> Result<GraphicListing> {
        self.call("scene.graphic.list", &serde_json::json!({})).await
    }

    /// Group the changes that follow into one undo step, until the next mark. This is what makes a drag of forty moves one Ctrl+Z.
    pub async fn scene_history_mark(&self, params: &MarkRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.history.mark", params).await
    }

    /// Read a collection bundle, a zip or the directory it unpacks to, and add its scenes to this one. Answers with a relink report for any asset that did not come across.
    pub async fn scene_import(&self, params: &ImportRequest2) -> Result<ImportedReport> {
        self.call("scene.import", params).await
    }

    /// Read an OBS Studio scene collection and add its scenes to this one. Send the file's text as `content` (what a page's file picker reads) or a `path` on the mixer's machine. With `add_sources: true` the sources the scenes draw are added through source.add, and the answer says which were added and why any were not.
    pub async fn scene_import_obs(&self, params: &ImportObsRequest) -> Result<ImportReport> {
        self.call("scene.import.obs", params).await
    }

    /// Put something on a scene's canvas. With no transform it lands in the next free cell, so a drop never needs a dialog.
    pub async fn scene_item_add(&self, params: &AddItemRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.item.add", params).await
    }

    /// Line items up on an edge: left, right, top, bottom, center-x or center-y.
    pub async fn scene_item_align(&self, params: &ItemsRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.item.align", params).await
    }

    /// Lay items out in a grid of `cols` columns.
    pub async fn scene_item_arrange_grid(&self, params: &ItemsRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.item.arrange_grid", params).await
    }

    /// Bind a geometry property to an expression over the collection's parameters, so changing a number moves everything that follows it.
    pub async fn scene_item_bind(&self, params: &BindRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.item.bind", params).await
    }

    /// Copy an item into another scene. The copy keeps the transform and the filters and gets a new id.
    pub async fn scene_item_copy(&self, params: &MoveItemRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.item.copy", params).await
    }

    /// Put items over the whole canvas, filling it and letting the overflow go.
    pub async fn scene_item_cover_canvas(&self, params: &ItemsRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.item.cover_canvas", params).await
    }

    /// Space items evenly between the two on the ends, horizontally or vertically.
    pub async fn scene_item_distribute(&self, params: &ItemsRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.item.distribute", params).await
    }

    /// Hang a filter on one item, so a camera keyed in one scene is not keyed in all of them.
    pub async fn scene_item_filter_add(&self, params: &AddItemFilterRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.item.filter.add", params).await
    }

    /// Take a filter off an item.
    pub async fn scene_item_filter_remove(&self, params: &ItemFilterRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.item.filter.remove", params).await
    }

    /// Change one of an item's filters, or turn it off without taking it out.
    pub async fn scene_item_filter_set(&self, params: &ItemFilterRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.item.filter.set", params).await
    }

    /// Put items over the whole canvas, keeping their aspect ratio inside it.
    pub async fn scene_item_fit_to_canvas(&self, params: &ItemsRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.item.fit_to_canvas", params).await
    }

    /// Put items into a group. The picture does not change.
    pub async fn scene_item_group(&self, params: &ItemsRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.item.group", params).await
    }

    /// Make items the same size as another one.
    pub async fn scene_item_match_size(&self, params: &ItemsRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.item.match_size", params).await
    }

    /// Move an item to another scene, keeping its transform and filters.
    pub async fn scene_item_move(&self, params: &MoveItemRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.item.move", params).await
    }

    /// Take an item off a scene.
    pub async fn scene_item_remove(&self, params: &ItemRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.item.remove", params).await
    }

    /// Move an item up or down the stack, between two named neighbours.
    pub async fn scene_item_reorder(&self, params: &ReorderRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.item.reorder", params).await
    }

    /// What one item type takes: a graphic's OGraf schema, or a source or filter plugin's settings schema. The same JSON Schema every client renders an inspector from.
    pub async fn scene_item_schema(&self, params: &ItemSchemaRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.item.schema", params).await
    }

    /// Assign an item's properties. Only the keys named move; the rest are left alone, so calling it twice with the same body changes nothing the second time.
    pub async fn scene_item_set(&self, params: &SetItemRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.item.set", params).await
    }

    /// Take a group apart, leaving every child exactly where it looked.
    pub async fn scene_item_ungroup(&self, params: &ItemRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.item.ungroup", params).await
    }

    /// Read one scene's geometry, to paste onto another.
    pub async fn scene_layout_copy(&self, params: &SceneRequest) -> Result<Layout> {
        self.call("scene.layout.copy", params).await
    }

    /// The layouts that ship with the core, with the parameters each one takes.
    pub async fn scene_layout_list(&self) -> Result<LayoutListing> {
        self.call("scene.layout.list", &serde_json::json!({})).await
    }

    /// Put one scene's geometry onto another's items, matched by name first and slot order second. Items that match nothing are left alone.
    pub async fn scene_layout_paste(&self, params: &LayoutClipboardRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.layout.paste", params).await
    }

    /// Every scene in the collection, with how many items it has, the sources it draws and whether it is armed.
    pub async fn scene_list(&self) -> Result<SceneListing> {
        self.call("scene.list", &serde_json::json!({})).await
    }

    /// The collection's typed parameters, readable without their values, so a client discovers what is fillable before filling it.
    pub async fn scene_params_get(&self) -> Result<BTreeMap<String, Value>> {
        self.call("scene.params.get", &serde_json::json!({})).await
    }

    /// Set the collection's parameter values, declaring any that are new. A `{{name}}` in any string property of any item follows them, so one call changes every lower third that uses it.
    pub async fn scene_params_set(&self, params: &ParamsRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.params.set", params).await
    }

    /// A still of the armed scene as base64 JPEG, the floor every client has.
    pub async fn scene_preview_frame(&self, params: &PreviewFrameRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.preview.frame", params).await
    }

    /// Arm a scene. The armed scene is the preview, and program.take with no argument takes it.
    pub async fn scene_preview_set(&self, params: &PreviewRequest) -> Result<BTreeMap<String, Value>> {
        self.call("scene.preview.set", params).await
    }

    /// Put back what undo took away.
    pub async fn scene_redo(&self) -> Result<HistoryStep> {
        self.call("scene.redo", &serde_json::json!({})).await
    }

    /// Delete a scene. What is on air is not touched.
    pub async fn scene_remove(&self, params: &SceneRequest) -> Result<SceneRemoved> {
        self.call("scene.remove", params).await
    }

    /// Change a scene's name, its colour, or both. Names and colours live on the document, so every client, the tally and an agent see the same ones.
    pub async fn scene_rename(&self, params: &RenameSceneRequest) -> Result<SceneView> {
        self.call("scene.rename", params).await
    }

    /// Throw the batch away. The document goes back to where it was when the batch opened.
    pub async fn scene_transaction_abort(&self) -> Result<BTreeMap<String, Value>> {
        self.call("scene.transaction.abort", &serde_json::json!({})).await
    }

    /// Start a batch. Everything until the commit applies on one frame or not at all, and undoes in one step.
    pub async fn scene_transaction_begin(&self) -> Result<BTreeMap<String, Value>> {
        self.call("scene.transaction.begin", &serde_json::json!({})).await
    }

    /// Apply the batch.
    pub async fn scene_transaction_commit(&self) -> Result<BTreeMap<String, Value>> {
        self.call("scene.transaction.commit", &serde_json::json!({})).await
    }

    /// Undo the last change. A drag marked with scene.history.mark undoes as one step.
    pub async fn scene_undo(&self) -> Result<HistoryStep> {
        self.call("scene.undo", &serde_json::json!({})).await
    }

    /// Overlaps, items off the canvas, safe area breaches and missing sources: what to fix before saying a scene is done.
    pub async fn scene_validate(&self, params: &ValidateRequest) -> Result<Validation> {
        self.call("scene.validate", params).await
    }

    /// A new scene with a presenter keyed in front of a background, and optionally a foreground such as a desk and a lower third area. Pictures from the media library become sources; the key colour is guessed from the camera.
    pub async fn scene_virtual_set(&self, params: &VirtualSetRequest) -> Result<VirtualSetAnswer> {
        self.call("scene.virtual_set", params).await
    }

    /// Make another show and start it: empty, a copy of a show (without its outputs, so nothing goes out twice), or from a project file.
    pub async fn show_add(&self, params: &ShowAddRequest) -> Result<Show> {
        self.call("show.add", params).await
    }

    /// Make many shows in one call, such as every channel of a headend. The whole batch is checked first. With dry_run (the default) nothing is made: the answer says what would be, what its renditions would cost and whether the governor would admit them. Without it, every show that fits is made and the rest are refused with why; a show is made whole or not at all.
    pub async fn show_add_many(&self, params: &ShowAddManyRequest) -> Result<ShowAddManyResult> {
        self.call("show.add_many", params).await
    }

    /// Every show on this machine: its name, whether it is running, what is on air, what its outputs send and what its process costs. `current` is the show a client reaches when it names none.
    pub async fn show_list(&self) -> Result<ShowList> {
        self.call("show.list", &serde_json::json!({})).await
    }

    /// Send a show without compositing to another place: an address (SRT, RTMP, UDP, RTP or RIST), a platform and its key, or hls://<name> to serve it as HLS from this port. Left without a rendition it copies the input's bytes; with one it is planned and admitted by the governor. The key is write only.
    pub async fn show_output_add(&self, params: &ShowOutputAddRequest) -> Result<Show> {
        self.call("show.output.add", params).await
    }

    /// Stop one output of a show without compositing and forget it, key and all.
    pub async fn show_output_remove(&self, params: &ShowOutputRemoveRequest) -> Result<Show> {
        self.call("show.output.remove", params).await
    }

    /// Change one output of a show without compositing, naming only what moves: another address, a new key, on or off, copy or a rendition.
    pub async fn show_output_set(&self, params: &ShowOutputSetRequest) -> Result<Show> {
        self.call("show.output.set", params).await
    }

    /// Stop a show and remove it with its folder. Refused for the last show and for main, the show the station was started with.
    pub async fn show_remove(&self, params: &IdRequest) -> Result<ShowRemoved> {
        self.call("show.remove", params).await
    }

    /// Stop and remove many shows. Each id that cannot go (main, or one not there) is refused with why, and the rest go.
    pub async fn show_remove_many(&self, params: &ShowRemoveManyRequest) -> Result<ShowRemoveManyResult> {
        self.call("show.remove_many", params).await
    }

    /// Give a show another name. Its id stays.
    pub async fn show_rename(&self, params: &ShowRenameRequest) -> Result<Show> {
        self.call("show.rename", params).await
    }

    /// Change a show's name, its input, or whether it composites. Turning compositing on starts a show process whose one source is the input and moves the outputs to it; turning it off hands them back to the direct host, when the show has one source and no scenes in use. A switch can take half a minute, so it answers at once with a task_id and the show as it is; task.get with that id carries this answer, with how long the outputs were off, once it is done.
    pub async fn show_set(&self, params: &ShowSetRequest) -> Result<ShowSetResult> {
        self.call("show.set", params).await
    }

    /// Start a stopped or failed show.
    pub async fn show_start(&self, params: &IdRequest) -> Result<Show> {
        self.call("show.start", params).await
    }

    /// Health, input numbers and each output's numbers for many shows in one read, from what the station already holds, so it is cheap to call every second for two hundred shows. `fields` narrows it to health, input or outputs.
    pub async fn show_stats(&self, params: &ShowStatsRequest) -> Result<ShowStatsList> {
        self.call("show.stats", params).await
    }

    /// Stop a show. It keeps its config, and stays stopped when the station starts again, until show.start.
    pub async fn show_stop(&self, params: &IdRequest) -> Result<Show> {
        self.call("show.stop", params).await
    }

    /// One JPEG: the whole contact sheet, the programme, or one source cut out of the mosaic.
    pub async fn snapshot_get(&self, params: &SnapshotRequest) -> Result<BTreeMap<String, Value>> {
        self.call("snapshot.get", params).await
    }

    /// Add a source while the mixer runs. Answers with the id it got and the whole source record.
    pub async fn source_add(&self, params: &AddSourceRequest) -> Result<SourceStatus> {
        self.call("source.add", params).await
    }

    /// Move a source's audio: the fader, the mute, and for a superimposed page the balance between its own sound and the videos under it.
    pub async fn source_audio_set(&self, params: &AudioSetParams) -> Result<SourceAudioState> {
        self.call("source.audio.set", params).await
    }

    /// Add another source like one the mixer has: the same address and settings under a new id. A client cannot do this with source.add, because the address it is shown has everything after the host cut off.
    pub async fn source_duplicate(&self, params: &DuplicateSourceRequest) -> Result<SourceStatus> {
        self.call("source.duplicate", params).await
    }

    /// One source. Refused with the ids that exist when there is no such source.
    pub async fn source_get(&self, params: &IdRequest) -> Result<SourceStatus> {
        self.call("source.get", params).await
    }

    /// Put sources in a tray folder. A tag for finding things, not a group on the canvas.
    pub async fn source_group(&self, params: &GroupSourcesRequest) -> Result<BTreeMap<String, Value>> {
        self.call("source.group", params).await
    }

    /// The colour to key a source on: the colour at a point of its picture, or with no point the green or blue screen it stands in front of.
    pub async fn source_key_color(&self, params: &KeyColorRequest) -> Result<KeyColor> {
        self.call("source.key_color", params).await
    }

    /// Every source, with its state, whether it has video and audio, and its fader.
    pub async fn source_list(&self) -> Result<Vec<SourceStatus>> {
        self.call("source.list", &serde_json::json!({})).await
    }

    /// Sources that are not running, and why: failed, could not be started (with the error and the action that fixes it), removed, or unknown. Pass the ids a scene draws, or none for every one the mixer knows about.
    pub async fn source_missing(&self, params: &MissingRequest) -> Result<Vec<MissingSource>> {
        self.call("source.missing", params).await
    }

    /// Remove a source. If it is on programme the mixer cuts to the slate first.
    pub async fn source_remove(&self, params: &IdRequest) -> Result<BTreeMap<String, Value>> {
        self.call("source.remove", params).await
    }

    /// Build a source's pipeline again now, rather than waiting for its next retry. For a source that could not be started or was removed, use source.restore.
    pub async fn source_restart(&self, params: &IdRequest) -> Result<SourceStatus> {
        self.call("source.restart", params).await
    }

    /// Put back a source that source.remove took away, as it was: same id, address, settings, fader and mute. The mixer remembers the last sixteen it removed, until it restarts.
    pub async fn source_restore(&self, params: &IdRequest) -> Result<SourceStatus> {
        self.call("source.restore", params).await
    }

    /// Move a seekable source to a position. Answers with where it actually landed.
    pub async fn source_seek(&self, params: &SeekParams) -> Result<SourcePositionState> {
        self.call("source.seek", params).await
    }

    /// Change a running source: its name and colour, its params, or where it runs. The name and colour live on the scene document. Moving a source between the core, a sidecar and a node is `place`; the programme keeps its frame rate across the move and the compositor covers the swap.
    pub async fn source_set(&self, params: &SetSourceRequest) -> Result<SourceStatus> {
        self.call("source.set", params).await
    }

    /// Ask a piece of long running work to stop. Cooperative: the answer says the request landed, not that the work has stopped yet.
    pub async fn task_cancel(&self, params: &TaskRequest) -> Result<BTreeMap<String, Value>> {
        self.call("task.cancel", params).await
    }

    /// How a piece of long running work is getting on, and its answer once it has one.
    pub async fn task_get(&self, params: &TaskRequest) -> Result<TaskView> {
        self.call("task.get", params).await
    }

    /// Every background job this core knows about, newest first.
    pub async fn task_list(&self) -> Result<Vec<TaskView>> {
        self.call("task.list", &serde_json::json!({})).await
    }

    /// Call one of a plugin's tools, in MCP's shape. The name is `<plugin>/<tool>`, or the bare tool name when only one plugin has it.
    pub async fn tool_call(&self, params: &ToolCallRequest) -> Result<BTreeMap<String, Value>> {
        self.call("tool.call", params).await
    }

    /// This show's health (its state and alarms, null in the first second) and the thresholds they are judged by.
    pub async fn vitals_get(&self) -> Result<BTreeMap<String, Value>> {
        self.call("vitals.get", &serde_json::json!({})).await
    }

    /// Change the alarm thresholds, or whether a mosaic is kept up for the black and freeze checks while nobody is looking. Fields left out keep their defaults; a duration of 0 switches that check off. Applies within a second.
    pub async fn vitals_set(&self, params: &VitalsConfig) -> Result<BTreeMap<String, Value>> {
        self.call("vitals.set", params).await
    }

}
