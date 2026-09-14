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
    /// rtmp:// or rtmps:// URL including the stream key.
    pub uri: String,
    /// Anything this build does not know a name for.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// `plugin.add`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AddPluginRequest {
    /// A local directory with `gmx-plugin.toml` at its root. Git, an index and
    /// a signed release are Phase 5; this takes a path.
    pub source: String,
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackendInfo {
    pub audio_decoder: String,
    pub audio_encoder: String,
    pub hardware_accelerated: bool,
    pub video_decoder: String,
    pub video_encoder: String,
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

/// `event/flush`: the end of a batch. A client renders here and not before.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Flush {
    /// The sequence number of the last event in the batch.
    pub seq: u64,
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

/// `program.history`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryRequest {
    /// How many takes to return, newest first. At most 100.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// An id on its own: `source.get`, `source.remove`, `output.remove`,
/// `output.reconnect`, `media.remove`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct IdRequest {
    pub id: String,
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MixerStatus {
    /// Present while an ad break is armed or running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ad: Option<AdStatus>,
    pub backend: BackendInfo,
    pub multiview: MultiviewStatus,
    pub outputs: Vec<OutputStatus>,
    /// Source currently on program, or None while the slate is showing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    /// Program pipeline running time. Cues are scheduled against this, not
    /// against wall clock, so a client can place a break on a known frame.
    pub running_time_ms: u64,
    pub sources: Vec<SourceStatus>,
    pub uptime_secs: u64,
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

pub type OutputState = String;
/// The values api_level 1 knows for [`OutputState`].
pub const OUTPUT_STATE_VALUES: &[&str] = &["connecting", "live", "reconnecting", "failed"];

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputStatus {
    pub id: String,
    /// Seconds of encoded data waiting in the pre-muxer queue. A number that
    /// climbs and stays high means the destination cannot keep up.
    pub queue_secs: f64,
    pub reconnects: u32,
    pub state: OutputState,
    pub uri_host: String,
    /// Anything this build does not know a name for.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
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
    /// Its MCP tools, as `gmx_<plugin>_<tool>`. Reachable with `search_tools`;
    /// never in the hot list.
    pub tools: Vec<String>,
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
    /// Its MCP tools, as `gmx_<plugin>_<tool>`. Reachable with `search_tools`;
    /// never in the hot list.
    pub tools: Vec<String>,
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

/// What `preview.close` answers with.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PreviewClosed {
    pub closed: bool,
    pub target: String,
}

/// `ext.preview`. Either `"full"`, `false`, or an object.
pub type PreviewExt = Value;

/// `preview.open {target}`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PreviewOpenRequest {
    /// `program`, or a source id.
    pub target: String,
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
    /// The previous source, which is what `program.revert` would take back to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
    /// Source on air, or null for the slate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    /// Programme pipeline running time, in milliseconds.
    pub running_time_ms: u64,
}

pub type ResponseFormat = String;
/// The values api_level 1 knows for [`ResponseFormat`].
pub const RESPONSE_FORMAT_VALUES: &[&str] = &["concise", "detailed"];

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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SaveRequest {
    /// The new preset's name. A slug: lower case letters, digits and hyphens.
    pub name: String,
    /// Where to write it. Defaults to `~/.godwinmix/presets/<name>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out: Option<String>,
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

/// `filter.set`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SetFilterRequest {
    pub id: String,
    /// The settings to apply. Only the keys named are changed.
    pub params: BTreeMap<String, Value>,
}

/// `plugin.settings.set`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SetSettingsRequest {
    pub id: String,
    /// Only the keys named are changed.
    pub settings: BTreeMap<String, Value>,
}

pub type Severity = String;
/// The values api_level 1 knows for [`Severity`].
pub const SEVERITY_VALUES: &[&str] = &["critical", "info", "warning", "error"];

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
    /// The scene to take, once scenes exist (11). Today a scene name is read
    /// as a one item scene, which is to say as a source id, and `source` wins
    /// when both are given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    /// Id of the source to put on air. Null or omitted cuts to the slate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
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
pub struct MediaChangedEvent {
    pub conversion: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<Severity>,
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

pub const METHODS: [MethodInfo; 63] = [
    MethodInfo { name: "adbreak.end", summary: "Cut a running ad short, or disarm one that is scheduled.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/adbreak/end")) },
    MethodInfo { name: "adbreak.start", summary: "Interrupt the programme with a clip, then rejoin live when it ends.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/adbreak/start")) },
    MethodInfo { name: "agent.state", summary: "The compact document written for agents: the programme, each source's state and a motion score saying how much its picture is changing.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/agent/state")) },
    MethodInfo { name: "codec.list", summary: "Every codec and element in the catalogue, which of them this machine actually has, and what it would pick.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/codecs")) },
    MethodInfo { name: "core.api", summary: "Every method, event and type as JSON Schema. The same document as protocol.json and `godwinmix --api-info`.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/core/api")) },
    MethodInfo { name: "core.doctor", summary: "The environment checks: GStreamer, the elements, the config, the disk and the ports. The same list `gmx doctor` prints.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/core/doctor")) },
    MethodInfo { name: "core.info", summary: "What this core is, what it can do, and where its edges are.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/core/info")) },
    MethodInfo { name: "core.session_log", summary: "The append only record of everything that happened, back as far as you ask.", scope: "admin", mutating: true, destructive: false, rest: Some(("GET", "/api/v1/core/session_log")) },
    MethodInfo { name: "core.shutdown", summary: "Stop the mixer, and with it the programme. Nothing else takes the show off air, so this is deliberately its own call.", scope: "admin", mutating: true, destructive: true, rest: Some(("POST", "/api/v1/core/shutdown")) },
    MethodInfo { name: "core.startup_report", summary: "How long each stage of the start took, and what was over the 250 ms mark.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/core/startup_report")) },
    MethodInfo { name: "core.status", summary: "The full state: programme, every source, every output, the multiview grid, the encoder backend and any ad break.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/core/status")) },
    MethodInfo { name: "core.subscribe", summary: "Subscribe to the event stream. WebSocket only: the core answers event/snapshot then deltas, ending every batch with event/flush.", scope: "read", mutating: false, destructive: false, rest: None },
    MethodInfo { name: "filter.add", summary: "Hang a filter on one source or on the programme, live.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/filters")) },
    MethodInfo { name: "filter.list", summary: "Every filter in place, with what it is and where it sits.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/filters")) },
    MethodInfo { name: "filter.remove", summary: "Take a filter out of the pipeline.", scope: "operate", mutating: true, destructive: true, rest: Some(("DELETE", "/api/v1/filters/{id}")) },
    MethodInfo { name: "filter.set", summary: "Change a filter's settings in place. A filter that cannot take the change while running says so rather than being restarted behind your back.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/filters/{id}/set")) },
    MethodInfo { name: "log.gst", summary: "Raise GStreamer's own debug categories for a while, then let them fall back on their own.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/log/gst")) },
    MethodInfo { name: "log.levels", summary: "Every log level override in force, and the GStreamer categories still raised.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/log/levels")) },
    MethodInfo { name: "log.set", summary: "Change one instance's or one module's log level while the mixer runs.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/log/set")) },
    MethodInfo { name: "media.convert", summary: "Transcode a library file to a web safe copy, in the background.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/media/{id}/convert")) },
    MethodInfo { name: "media.list", summary: "The clips in the library, with durations and whether each has audio.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/media")) },
    MethodInfo { name: "media.remove", summary: "Delete a library file and its converted copy. Refused while it is a live source.", scope: "operate", mutating: true, destructive: true, rest: Some(("DELETE", "/api/v1/media/{id}")) },
    MethodInfo { name: "media.upload", summary: "Stream a file into the library. HTTP only: the body is the file.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/media/upload")) },
    MethodInfo { name: "output.add", summary: "Send the programme to another destination. The encoder is shared, so adding one costs nothing on air.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/outputs")) },
    MethodInfo { name: "output.get", summary: "One destination.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/outputs/{id}")) },
    MethodInfo { name: "output.list", summary: "Every destination, with its state, reconnect count and how much is buffered.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/outputs")) },
    MethodInfo { name: "output.reconnect", summary: "Drop and re-establish one destination's connection now, without waiting for its reconnect policy.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/outputs/{id}/reconnect")) },
    MethodInfo { name: "output.remove", summary: "Stop sending to a destination and forget it. Other outputs are unaffected.", scope: "operate", mutating: true, destructive: true, rest: Some(("DELETE", "/api/v1/outputs/{id}")) },
    MethodInfo { name: "pipeline.clock", summary: "The clock every pipeline is running against, and how far each one has got.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/pipeline/clock")) },
    MethodInfo { name: "pipeline.dot", summary: "One pipeline as a graphviz graph: every element, every pad and the caps negotiated between them.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/pipeline/dot")) },
    MethodInfo { name: "pipeline.latency", summary: "How much delay one pipeline is carrying, and which stage put it there.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/pipeline/latency")) },
    MethodInfo { name: "pipeline.list", summary: "Every pipeline running right now, by the name the other pipeline methods accept.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/pipeline/list")) },
    MethodInfo { name: "pipeline.queues", summary: "Every queue in one pipeline with how full it is, fullest first. A queue that stays full is where the trouble is.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/pipeline/queues")) },
    MethodInfo { name: "plugin.add", summary: "Install a plugin from a local directory, while live. The directory is the one with gmx-plugin.toml at its root.", scope: "admin", mutating: true, destructive: true, rest: Some(("POST", "/api/v1/plugins")) },
    MethodInfo { name: "plugin.describe", summary: "One plugin in full: its manifest, the settings schema of every provide, and the description from each SKILL.md.", scope: "read", mutating: false, destructive: false, rest: Some(("POST", "/api/v1/plugins/{id}/describe")) },
    MethodInfo { name: "plugin.disable", summary: "Turn a plugin off without uninstalling it. It registers nothing and runs no process until it is enabled again.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/plugins/{id}/disable")) },
    MethodInfo { name: "plugin.enable", summary: "Turn a plugin back on. It registers what it declares and its instances start.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/plugins/{id}/enable")) },
    MethodInfo { name: "plugin.list", summary: "Every plugin installed, with what it provides and what each running instance is costing in cpu, memory, latency, dropped buffers and restarts.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/plugins")) },
    MethodInfo { name: "plugin.reload", summary: "Read a plugin's directory again and swap its running instances one at a time, with the freeze frame covering each.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/plugins/{id}/reload")) },
    MethodInfo { name: "plugin.remove", summary: "Uninstall a plugin and unwind everything it registered: its provides, its tools, its panels, its hooks and its discovery matchers.", scope: "admin", mutating: true, destructive: true, rest: Some(("DELETE", "/api/v1/plugins/{id}")) },
    MethodInfo { name: "plugin.settings.get", summary: "A plugin's settings as they stand, with its schema beside them.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/plugins/{id}/settings")) },
    MethodInfo { name: "plugin.settings.set", summary: "Change a plugin's settings. A plugin that cannot take a change while running says so rather than being restarted behind your back.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/plugins/{id}/settings")) },
    MethodInfo { name: "plugin.stats", summary: "Per instance cpu, memory, media latency, dropped buffers and restarts, refreshed once a second.", scope: "read", mutating: false, destructive: false, rest: Some(("POST", "/api/v1/plugins/{id}/stats")) },
    MethodInfo { name: "preset.apply", summary: "Put a preset on this core: its config, its scenes, its layout, its theme and its gallery mode. Pass dry_run to get the plan and write nothing.", scope: "admin", mutating: true, destructive: true, rest: Some(("POST", "/api/v1/preset/apply")) },
    MethodInfo { name: "preset.list", summary: "Every preset this core can apply: the six built in, plus anything installed beside the binary or under ~/.godwinmix/presets.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/preset/list")) },
    MethodInfo { name: "preset.save", summary: "Turn this core's working setup into a preset directory somebody else can apply. Stream keys and the control token are replaced with placeholders.", scope: "admin", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/preset/save")) },
    MethodInfo { name: "preview.close", summary: "Give up a raw frame socket. The socket goes when the last holder closes it.", scope: "read", mutating: false, destructive: false, rest: Some(("POST", "/api/v1/preview/close")) },
    MethodInfo { name: "preview.open", summary: "Open a raw frame socket on this machine for a source or the programme, and answer with its path. No encode anywhere: a client on the same host reads the frames the mixer already has. Close it with preview.close.", scope: "read", mutating: false, destructive: false, rest: Some(("POST", "/api/v1/preview/open")) },
    MethodInfo { name: "program.get", summary: "What is on air, the programme running time, and what revert would go back to.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/program")) },
    MethodInfo { name: "program.golive", summary: "One call to put a web page on air: add the page, add the destination, and take the page as soon as it renders.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/program/golive")) },
    MethodInfo { name: "program.history", summary: "The last hundred takes, newest first, with the token that asked for each.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/program/history")) },
    MethodInfo { name: "program.revert", summary: "Take back to the shot before this one.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/program/revert")) },
    MethodInfo { name: "program.take", summary: "Put a source on programme. The cut is instant and the outgoing stream is not disturbed.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/program/take")) },
    MethodInfo { name: "snapshot.get", summary: "One JPEG: the whole contact sheet, the programme, or one source cut out of the mosaic.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/snapshot/{id}")) },
    MethodInfo { name: "source.add", summary: "Add a source while the mixer runs. Answers with the id it got and the whole source record.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/sources")) },
    MethodInfo { name: "source.audio.set", summary: "Move a source's audio: the fader, the mute, and for a superimposed page the balance between its own sound and the videos under it.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/sources/{id}/audio")) },
    MethodInfo { name: "source.get", summary: "One source. Refused with the ids that exist when there is no such source.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/sources/{id}")) },
    MethodInfo { name: "source.list", summary: "Every source, with its state, whether it has video and audio, and its fader.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/sources")) },
    MethodInfo { name: "source.remove", summary: "Remove a source. If it is on programme the mixer cuts to the slate first.", scope: "operate", mutating: true, destructive: true, rest: Some(("DELETE", "/api/v1/sources/{id}")) },
    MethodInfo { name: "source.seek", summary: "Move a seekable source to a position. Answers with where it actually landed.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/sources/{id}/seek")) },
    MethodInfo { name: "task.cancel", summary: "Ask a piece of long running work to stop. Cooperative: the answer says the request landed, not that the work has stopped yet.", scope: "operate", mutating: true, destructive: false, rest: Some(("POST", "/api/v1/task/cancel")) },
    MethodInfo { name: "task.get", summary: "How a piece of long running work is getting on, and its answer once it has one.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/task")) },
    MethodInfo { name: "task.list", summary: "Every background job this core knows about, newest first.", scope: "read", mutating: false, destructive: false, rest: Some(("GET", "/api/v1/task/list")) },
];

pub const EVENT_NAMES: [&str; 17] = [
    "snapshot",
    "program.took",
    "source.state",
    "source.position",
    "output.state",
    "adbreak.changed",
    "ui.changed",
    "media.changed",
    "meters",
    "tally",
    "alert",
    "telemetry",
    "agent.state",
    "multiview.layout",
    "multiview.frame",
    "resync",
    "flush",
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
    /// A file in the library was uploaded, deleted, or its conversion moved on.
    MediaChanged(MediaChangedEvent),
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
    /// This client fell behind and events were dropped. Re-subscribe for a fresh snapshot; nothing between from_seq and the new snapshot arrives.
    Resync(Resync),
    /// The end of a batch. Render here and not before, so a client never paints half an update.
    Flush(Flush),
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
            "media.changed" => match serde_json::from_value(params.clone()) {
                Ok(payload) => Event::MediaChanged(payload),
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
            _ => Event::Other { name: pattern.to_string(), params },
        }
    }

    /// The name after `event/`, whatever the variant.
    pub fn name(&self) -> &str {
        match self {
            Event::Snapshot(_) => "snapshot",
            Event::ProgramTook(_) => "program.took",
            Event::SourceState(_) => "source.state",
            Event::SourcePosition(_) => "source.position",
            Event::OutputState(_) => "output.state",
            Event::AdbreakChanged(_) => "adbreak.changed",
            Event::UiChanged(_) => "ui.changed",
            Event::MediaChanged(_) => "media.changed",
            Event::Meters(_) => "meters",
            Event::Tally(_) => "tally",
            Event::Alert(_) => "alert",
            Event::Telemetry(_) => "telemetry",
            Event::AgentState(_) => "agent.state",
            Event::MultiviewLayout(_) => "multiview.layout",
            Event::MultiviewFrame(_) => "multiview.frame",
            Event::Resync(_) => "resync",
            Event::Flush(_) => "flush",
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

    /// Every codec and element in the catalogue, which of them this machine actually has, and what it would pick.
    pub async fn codec_list(&self) -> Result<BTreeMap<String, Value>> {
        self.call("codec.list", &serde_json::json!({})).await
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

    /// Install a plugin from a local directory, while live. The directory is the one with gmx-plugin.toml at its root.
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

    /// Put a source on programme. The cut is instant and the outgoing stream is not disturbed.
    pub async fn program_take(&self, params: &TakeRequest) -> Result<ProgramState> {
        self.call("program.take", params).await
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

    /// One source. Refused with the ids that exist when there is no such source.
    pub async fn source_get(&self, params: &IdRequest) -> Result<SourceStatus> {
        self.call("source.get", params).await
    }

    /// Every source, with its state, whether it has video and audio, and its fader.
    pub async fn source_list(&self) -> Result<Vec<SourceStatus>> {
        self.call("source.list", &serde_json::json!({})).await
    }

    /// Remove a source. If it is on programme the mixer cuts to the slate first.
    pub async fn source_remove(&self, params: &IdRequest) -> Result<BTreeMap<String, Value>> {
        self.call("source.remove", params).await
    }

    /// Move a seekable source to a position. Answers with where it actually landed.
    pub async fn source_seek(&self, params: &SeekParams) -> Result<SourcePositionState> {
        self.call("source.seek", params).await
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

}
