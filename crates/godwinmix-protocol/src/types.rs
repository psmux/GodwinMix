//! Every status, event and plain data type the control surface carries.
//!
//! These moved out of `state.rs` so that one module is the single source of
//! truth for the wire format. `state.rs` re-exports all of them, so existing
//! `crate::state::MixerStatus` paths keep working; new code should say
//! `crate::MixerStatus`.
//!
//! Every type derives `JsonSchema` as well as `Serialize` and `Deserialize`,
//! which is what lets `core.api` and `protocol.json` be generated rather than
//! written by hand and left to drift.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub type SourceId = String;
pub type OutputId = String;

/// Per kind data a plugin hangs off a status record.
///
/// Flattened into the object, so a plugin's `codec` or `device` key sits
/// beside `id` and `state` rather than under a wrapper. Absent from the JSON
/// when empty, which is every source the core builds itself today.
pub type Extra = Map<String, Value>;

fn extra_is_empty(extra: &Extra) -> bool {
    extra.is_empty()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SourceState {
    /// Pipeline is up but no media has arrived yet.
    Connecting,
    /// Buffers arriving within the stall timeout.
    Live,
    /// Was live, then went quiet. Its program pad is held at alpha 0 so the
    /// slate shows through rather than a frozen frame.
    Stalled,
    /// The input pipeline errored. A retry is scheduled.
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum OutputState {
    Connecting,
    Live,
    Reconnecting,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SourceStatus {
    pub id: SourceId,
    pub name: String,
    pub uri: String,
    pub state: SourceState,
    pub has_video: bool,
    pub has_audio: bool,
    /// Index into the multiview grid, or None while the source has no cell.
    pub cell: Option<u32>,
    /// Milliseconds since the last video buffer, or None if none has arrived.
    pub video_idle_ms: Option<u64>,
    /// Same for audio. `None` here while `has_audio` is true means the source
    /// advertised an audio track that never produced a decoded sample.
    pub audio_idle_ms: Option<u64>,
    /// The operator's fader for this source, 0.0 silent through 1.0 unity to a
    /// ceiling of 10.0. Read back off the volume element rather than remembered,
    /// so what the UI shows is what the pipeline is doing.
    #[serde(default = "unity_gain")]
    pub gain: f64,
    /// Muted by the operator. Held apart from the fader so that unmuting returns
    /// the source to where it was rather than to unity.
    #[serde(default)]
    pub muted: bool,
    /// True when this source can be scrubbed. A file can be. A camera, an RTMP
    /// feed or a page cannot, and asking one to is a mistake worth refusing
    /// rather than quietly doing nothing.
    #[serde(default)]
    pub seekable: bool,
    /// Where this source has got to, and how long it runs, in milliseconds.
    /// `None` on anything not seekable, and on a seekable source whose duration
    /// the demuxer has not worked out yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Whatever only this kind of source has: a camera's resolution, a file's
    /// codec, a page's `superimposed` flag, a remote node's name.
    ///
    /// `superimposed` and `audio` used to be fields of this universal shape,
    /// which meant a kind could not report anything without a core release and
    /// every client carried fields that were false for almost every source.
    /// They are written here now, flattened, so the JSON on the wire is exactly
    /// what it was: `superimposed` is still a top level boolean and `audio` is
    /// still a top level object. What changed is that a new kind adds its own
    /// keys without touching this struct.
    #[serde(flatten, default, skip_serializing_if = "extra_is_empty")]
    pub extra: Extra,
}

impl SourceStatus {
    /// Put one kind specific field in the extras, dropping it if it does not
    /// serialise. A field that cannot be written is a bug in the kind, not a
    /// reason to fail a status snapshot the operator is waiting on.
    pub fn put_extra(&mut self, key: &str, value: impl Serialize) {
        match serde_json::to_value(value) {
            Ok(v) => {
                self.extra.insert(key.to_string(), v);
            }
            Err(e) => tracing::warn!(key, ?e, "a source's extra field could not be written"),
        }
    }

    /// Read one back, for a client or a test.
    pub fn extra(&self, key: &str) -> Option<&Value> {
        self.extra.get(key)
    }

    /// True when this website source is running with its media decoded outside
    /// the browser and the page drawn over it.
    pub fn superimposed(&self) -> bool {
        self.extra("superimposed").and_then(|v| v.as_bool()).unwrap_or(false)
    }
}

/// A fader that has never been moved sits at unity. Spelled out as a serde
/// default so a snapshot written before the fader existed parses as a desk with
/// every fader up, which is where those sources actually were.
pub fn unity_gain() -> f64 {
    1.0
}

/// The gains a superimposed source is currently running with.
///
/// 1.0 is unity, 0.0 is silent, and the ceiling is 10.0. Reported rather than
/// remembered: these are read back off the volume elements, so what the UI
/// shows is what the pipeline is doing, including any clamping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SourceAudio {
    /// The page's own sound, which is the commentary and whatever the page
    /// plays itself.
    pub page: f64,
    /// One per video the mixer decodes underneath, in the order the page
    /// handed them over.
    pub media: Vec<f64>,
}

/// What a source's audio controls read back as, which is what the audio
/// endpoint answers with.
///
/// Wider than `SourceAudio` because the fader and the mute apply to every
/// source, while the page and media balance belongs only to a superimposed one.
/// Every number here is read off the elements after the request landed, so a
/// request whose gain was clamped answers with the gain that took effect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SourceAudioState {
    pub gain: f64,
    pub muted: bool,
    /// Absent on anything but a superimposed source, which is the only kind
    /// with separate sounds to balance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<Vec<f64>>,
}

/// Where a seekable source has got to, which is what the seek endpoint answers
/// with.
///
/// Both numbers are read back off the pipeline after the seek has landed, not
/// taken from the request. A seek snaps to a key unit, so the frame an operator
/// asked for and the frame they got are rarely the same millisecond, and a
/// scrubber drawn from the request would sit a little away from the picture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SourcePositionState {
    pub position_ms: u64,
    /// Absent while the demuxer has not worked the duration out yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OutputStatus {
    pub id: OutputId,
    pub uri_host: String,
    pub state: OutputState,
    pub reconnects: u32,
    /// Seconds of encoded data waiting in the pre-muxer queue. A number that
    /// climbs and stays high means the destination cannot keep up.
    pub queue_secs: f64,
    /// Per kind data from whatever plugin owns this output. Empty for the
    /// RTMP outputs the core builds itself.
    #[serde(flatten, default, skip_serializing_if = "extra_is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MultiviewStatus {
    pub enabled: bool,
    pub width: i32,
    pub height: i32,
    pub cols: u32,
    pub rows: u32,
    /// Cell index to source id, in reading order. Cell 0 is the program return
    /// when it is enabled.
    pub cells: Vec<CellAssignment>,
    /// Frame rate of the mosaic, so the UI can size its own expectations.
    pub fps: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CellAssignment {
    pub index: u32,
    /// None means this cell is the program return.
    pub source: Option<SourceId>,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// An ad break, either armed for a future cue or currently on air.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AdStatus {
    pub uri: String,
    /// Source to return to when the ad ends. None returns to the slate.
    pub return_to: Option<SourceId>,
    /// False while it is prerolled and waiting for its cue.
    pub on_air: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MixerStatus {
    /// Source currently on program, or None while the slate is showing. A
    /// scene of one full canvas item reports that item's source here too, so
    /// anything written against this before scenes existed still reads.
    pub program: Option<SourceId>,
    /// The scene on air, when one was taken by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    pub sources: Vec<SourceStatus>,
    pub outputs: Vec<OutputStatus>,
    pub multiview: MultiviewStatus,
    pub uptime_secs: u64,
    /// Program pipeline running time. Cues are scheduled against this, not
    /// against wall clock, so a client can place a break on a known frame.
    pub running_time_ms: u64,
    pub backend: BackendInfo,
    /// Present while an ad break is armed or running.
    pub ad: Option<AdStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BackendInfo {
    pub video_decoder: String,
    pub video_encoder: String,
    pub audio_decoder: String,
    pub audio_encoder: String,
    pub hardware_accelerated: bool,
}

/// Pushed to every connected UI as it happens.
///
/// The `type` tag is the legacy `/ws` name. `/rpc` sends the same payload
/// under the JSON-RPC method name `event/<name>` from the events table in
/// `api::protocol`, so the two surfaces never carry different data.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// A full snapshot. Sent on connect and after any structural change.
    Status(Box<MixerStatus>),
    /// The program source changed. Carries the running time the cut landed on
    /// so the UI can show how close a scheduled take was to its mark.
    Took {
        source: Option<SourceId>,
        /// The scene that was taken, when a scene was named. A bare source id
        /// is shorthand for a one item full canvas scene and leaves this out.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scene: Option<String>,
        at_running_time_ms: u64,
    },
    /// The armed scene changed, or was cleared. Nothing is composited for a
    /// preview until a client asks for one, so this costs a message.
    PreviewChanged { scene: Option<String> },
    SourceStateChanged { source: SourceId, state: SourceState },
    OutputStateChanged { output: OutputId, state: OutputState, reconnects: u32 },
    /// An ad break started or ended.
    AdBreakChanged { ad: Option<AdStatus> },
    /// Peak level per channel, in dBFS, from the program bus. The mosaic
    /// carries no audio, so this is how an operator confirms that what is
    /// going out actually has sound on it.
    AudioLevel { peak_db: Vec<f64> },
    /// Peak level per channel for one source, in dBFS, measured after the
    /// operator's fader and before the mute. The mosaic carries no audio, so
    /// this is what puts a meter beside each picture.
    SourceAudioLevel { source: SourceId, peak_db: Vec<f64> },
    /// How far through a seekable source has got. Sent a few times a second for
    /// those sources only, because a camera has no position to report and a
    /// scrubber updated twice a minute is worse than no scrubber.
    SourcePosition { source: SourceId, position_ms: u64, duration_ms: Option<u64> },
    /// Something went wrong that the operator should see.
    Alert { severity: Severity, message: String },
    /// A file in the media library changed: uploaded, deleted, or its
    /// conversion moved on. The UI refetches the media listing rather than
    /// being sent the whole item, because the listing is the one place a
    /// converted copy gets folded onto its original.
    MediaChanged { name: String, conversion: Option<ConversionState> },
    /// The surface defaults changed: a preset was applied, or an operator set
    /// them by hand. Clients that render a UI re-read them and nothing else
    /// happens; the programme is untouched.
    UiChanged { ui: UiDefaults },
    /// A hook did not get its say: it timed out, or the thing behind it could
    /// not be reached. The take went ahead anyway. See 03 section 8.
    HookBlocked { hook: String, plugin: String, reason: String },
}

/// What a surface starts with: the layout, the theme and the gallery mode.
///
/// Chosen by a preset (`preset.apply`), carried in `core.info` and pushed as
/// `event/ui.changed`. None of it changes what the core does. It exists so the
/// first page a volunteer sees is the one their preset chose rather than the
/// one the last person to use this browser chose. 05 section 3b is where the
/// four gallery modes are defined.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UiDefaults {
    /// The preset that set these, so a surface knows one has been applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// A theme id the surface resolves, for example `dark` or `calm`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// `live`, `snapshot`, `icon` or `label`. Absent means the surface asks
    /// the machine, which is what `gmx doctor` proposes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gallery: Option<String>,
    /// Slot to panels, top to bottom. Empty means the surface's own default.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub layout: std::collections::BTreeMap<String, Vec<String>>,
}

impl UiDefaults {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
    /// Loud enough to page someone. Nothing raises it yet; it is here so the
    /// severity ladder in `protocol.json` is the one 03 section 6 names.
    Critical,
}

/// The canvas every source is scaled onto and every output leaves by.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub struct CanvasInfo {
    pub width: i32,
    pub height: i32,
    pub fps: i32,
}

/// The ceilings a client should plan against rather than discover by being
/// refused.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Limits {
    /// Largest upload the media endpoint accepts, in bytes.
    pub max_upload_bytes: usize,
    /// Loudest a fader can be asked for. Out of range is clamped to this
    /// rather than refused.
    pub max_gain: f64,
    /// Longest any method blocks before it answers. The tightest client
    /// default in the wild, so nothing here can time out a client that used
    /// its own.
    pub max_call_secs: u64,
    /// Longest `idempotency_key` accepted, in bytes.
    pub max_idempotency_key_bytes: usize,
    /// How many events the bus holds before a slow client is told to resync.
    pub event_queue: usize,
}

/// What the calling token is allowed to do, echoed back so a surface can grey
/// out what it cannot reach instead of discovering it at the first refusal.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TokenInfo {
    pub id: String,
    pub scopes: Vec<String>,
    /// "none" or "required": whether destructive calls need a confirm token.
    pub confirm: String,
    pub rehearsal: bool,
    /// MCP tool profile this token is meant for: "standard" or "minimal".
    pub profile: String,
}

/// `core.info`: what this core is, what it can do, and where its edges are.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CoreInfo {
    /// Always "godwinmix".
    pub core: String,
    /// The build's own version, as in Cargo.toml.
    pub version: String,
    pub api_level: u32,
    pub api_compatible: u32,
    /// Feature strings a client can branch on: multiview, snapshot, uploads,
    /// mcp, browser, exec-sources, rehearsal, tokens.
    pub features: Vec<String>,
    pub limits: Limits,
    pub canvas: CanvasInfo,
    /// Present when the request carried a token the core recognises.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<TokenInfo>,
    /// True when the core was started with `--rehearsal`, which refuses
    /// `output.add` and accepts rehearsal tokens.
    pub rehearsal: bool,
    /// What a surface should start with, when a preset chose it. Absent on a
    /// core no preset has been applied to, which is what puts the welcome
    /// panel up in the reference UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiDefaults>,
}

/// What every mutating method answers with alongside its result object.
///
/// `replayed` is Stripe's model: a retry with the same `idempotency_key` gets
/// the first call's body back with this set, so a client that timed out and
/// tried again never doubles a take or a source.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MutationMeta {
    /// True when this body came out of the idempotency cache.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub replayed: bool,
    /// Whether retrying this exact call is safe and worth doing.
    pub should_retry: bool,
}

/// What a `dry_run: true` call answers with instead of doing the work.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DryRun {
    /// False when the live state already matches what was asked for.
    pub would_change: bool,
    /// One line per change, in the order they would be applied.
    pub diff: Vec<String>,
    /// Echoed so a caller reading a log knows what was being asked.
    pub method: String,
}

/// Strip credentials out of an RTMP URI before it goes anywhere near the UI.
/// Stream keys live in the path of most CDN ingest URLs and must not be shown.
pub fn safe_uri_label(uri: &str) -> String {
    match uri.split_once("://") {
        Some((scheme, rest)) => {
            let hostport = rest.split('/').next().unwrap_or(rest);
            let host = hostport.rsplit('@').next().unwrap_or(hostport);
            format!("{scheme}://{host}/…")
        }
        None => "…".to_string(),
    }
}

// Where a media file's conversion to a web safe copy has got to. Carried on
// `event/media.changed`, so it is part of the wire contract even though the
// conversion itself is the engine's business.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversionPhase {
    Running,
    Done,
    Failed,
}

/// One conversion, in flight or remembered after it finished.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
pub struct ConversionState {
    pub state: ConversionPhase,
    /// 0.0 to 1.0, position over duration, both read off the pipeline.
    pub progress: f64,
    /// Set only on `failed`, shown to the operator verbatim: "no AAC encoder
    /// available" is worth more than "conversion failed".
    pub error: Option<String>,
    /// The `.web.mp4` name, on `done`.
    pub output: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `extra` map is the seam a source kind fills in. It has to round
    /// trip, sit flat beside the core's own fields, and vanish when empty so
    /// that today's status bytes do not change at all.
    #[test]
    fn per_kind_data_rides_flat_and_disappears_when_empty() {
        let mut s = SourceStatus {
            id: "cam1".into(),
            name: "Camera 1".into(),
            uri: "rtmp://host/live/cam1".into(),
            state: SourceState::Live,
            has_video: true,
            has_audio: true,
            cell: Some(2),
            video_idle_ms: Some(20),
            audio_idle_ms: Some(20),
            gain: 1.0,
            muted: false,
            seekable: false,
            position_ms: None,
            duration_ms: None,
            extra: Extra::new(),
        };
        let v = serde_json::to_value(&s).unwrap();
        assert!(v.get("extra").is_none(), "an empty extra writes no key at all");

        s.extra.insert("codec".into(), Value::String("h264".into()));
        s.extra.insert("device".into(), serde_json::json!({ "index": 0 }));
        let v = serde_json::to_value(&s).unwrap();
        // Flat, beside `id`, not nested under a wrapper.
        assert_eq!(v["codec"], "h264");
        assert_eq!(v["device"]["index"], 0);
        assert_eq!(v["id"], "cam1");

        let back: SourceStatus = serde_json::from_value(v).unwrap();
        assert_eq!(back.extra.get("codec").unwrap(), "h264");
        // The core's own fields do not leak into extra.
        assert!(!back.extra.contains_key("id"));
        assert!(!back.extra.contains_key("gain"));
    }

    #[test]
    fn an_output_carries_per_kind_data_too() {
        let o = OutputStatus {
            id: "yt".into(),
            uri_host: "rtmp://a.rtmp.youtube.com/…".into(),
            state: OutputState::Live,
            reconnects: 0,
            queue_secs: 0.2,
            extra: Extra::new(),
        };
        let v = serde_json::to_value(&o).unwrap();
        assert!(v.get("extra").is_none());
        let o = OutputStatus {
            extra: [("bitrate_kbps".to_string(), Value::from(4500))].into_iter().collect(),
            ..o
        };
        let v = serde_json::to_value(&o).unwrap();
        assert_eq!(v["bitrate_kbps"], 4500);
        let back: OutputStatus = serde_json::from_value(v).unwrap();
        assert_eq!(back.extra.get("bitrate_kbps").unwrap(), 4500);
        assert!(!back.extra.contains_key("queue_secs"));
    }

    #[test]
    fn stream_keys_never_reach_the_ui() {
        assert_eq!(
            safe_uri_label("rtmp://a.rtmp.youtube.com/live2/abcd-secret-key"),
            "rtmp://a.rtmp.youtube.com/…"
        );
        assert_eq!(
            safe_uri_label("rtmp://user:password@ingest.example.com/app/key"),
            "rtmp://ingest.example.com/…"
        );
        assert_eq!(safe_uri_label("garbage"), "…");
    }
}
