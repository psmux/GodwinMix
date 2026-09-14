//! Every request body, in one place, with the derives that let the schema be
//! generated rather than written twice.
//!
//! control.rs parses these, ctl.rs builds them, mcp.rs turns their schemas
//! into tool input schemas and `protocol.json` publishes them. Nothing
//! assembles a body by hand any more.
//!
//! Four keys are accepted on every method and are handled by the dispatcher
//! rather than by any one of these structs, so they are documented here and
//! nowhere else:
//!
//! * `trace_id`: carried through the answer and the log line. Generated when
//!   absent, or taken from the W3C `traceparent` header over HTTP.
//! * `idempotency_key`: on every mutating method. Honoured for 24 hours.
//! * `dry_run`: on every destructive method. Answers the diff, changes nothing.
//! * `confirm`: the token from a `-32020` refusal, valid for 30 seconds.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// `program.take`: put a source on programme.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct TakeRequest {
    /// Id of the source to put on air.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The scene to take, by name or by id. `source` wins when both are
    /// given; with neither, the armed scene goes on air.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    /// `cut` in this build.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<String>,
    /// Programme running time to land the cut on, in milliseconds. Omit for
    /// immediate. Read the current running time from `core.info` or a status
    /// snapshot first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_running_time_ms: Option<u64>,
}

/// The transitions this build runs. Widening this list is Phase 6.
pub const TRANSITIONS: &[&str] = &["cut"];

impl TakeRequest {
    /// The source named, if one was, trimmed.
    pub fn source_id(&self) -> Option<String> {
        self.source.clone().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    }

    /// The scene named, if one was, trimmed.
    pub fn scene_name(&self) -> Option<String> {
        self.scene.clone().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    }

    /// What a client written before scenes existed meant: `source` when given,
    /// otherwise the scene name read as a source id.
    pub fn target(&self) -> Option<String> {
        self.source_id().or_else(|| self.scene_name())
    }

    /// `Ok` for a transition this build runs, or the list of the ones it does.
    pub fn check_transition(&self) -> Result<(), String> {
        match self.transition.as_deref().map(str::trim) {
            None | Some("") | Some("cut") => Ok(()),
            Some(other) => Err(format!(
                "this build has no transition called {other:?}. It has: {}. Transitions                  between scenes land in a later release; leave `transition` out for a cut.",
                TRANSITIONS.join(", ")
            )),
        }
    }
}

/// `source.add`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct AddSourceRequest {
    /// Stable id used by `program.take` and `source.remove`. Lowercase
    /// letters, digits and dashes. Derived from the name or the host when
    /// omitted, with a numeric suffix if that is taken.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Name shown to an operator. Defaults to the host of the URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Stream URL, file path, or with kind "web" the address of a page.
    pub uri: String,
    /// "web" renders the URL as a page in the browser sidecar, the same as
    /// writing `web+` in front of it. "auto" or omitted works the protocol out
    /// from the URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Websites only: "auto" lets the mixer decode the page's own video
    /// outside the browser and draw the page over the top, which saves about a
    /// CPU core. "off" is the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superimpose: Option<String>,
    /// Anything a source kind of its own understands. Passed through to the
    /// source config untouched, which is the seam the plugin work lands on.
    #[serde(flatten, default, skip_serializing_if = "Map::is_empty")]
    pub params: Map<String, Value>,
}

/// `source.audio.set`. Every part optional, because a surface moves one
/// control at a time and has no reason to restate the others.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct AudioRequest {
    /// The operator's fader for the whole source, 0.0 to 10.0. Omit to leave
    /// it where it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gain: Option<f64>,
    /// Mute the whole source. Held apart from the fader, so unmuting comes
    /// back to the level that was set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    /// Gain on a superimposed page's own sound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<f64>,
    /// Gain per video underneath, by position. A null entry, or a list shorter
    /// than the number of videos, leaves those alone: `[null, 0.0]` silences
    /// the second video and touches nothing else.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<Option<f64>>,
}

/// `source.seek`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SeekRequest {
    /// Milliseconds from the start of the clip. Off either end is clamped
    /// rather than refused, so a scrubber flicked past the end lands there.
    pub position_ms: f64,
}

/// `output.add`. The id and the URL are the whole of it for an RTMP
/// destination; anything else a kind understands rides in `params`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct AddOutputRequest {
    /// Stable id for this destination.
    pub id: String,
    /// rtmp:// or rtmps:// URL including the stream key.
    pub uri: String,
    /// Reconnect policy: "own" retries quickly, for servers you run; "cdn"
    /// backs off harder, for platforms that penalise hammering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
    /// Passed through to the output config untouched.
    #[serde(flatten, default, skip_serializing_if = "Map::is_empty")]
    pub params: Map<String, Value>,
}

impl AddOutputRequest {
    /// The JSON an `OutputConfig` is built from. Written here rather than in
    /// control.rs so the one place that knows the mapping is the api module.
    pub fn to_config_json(&self) -> Value {
        let mut map = self.params.clone();
        map.insert("id".into(), Value::String(self.id.clone()));
        map.insert("uri".into(), Value::String(self.uri.clone()));
        if let Some(p) = &self.policy {
            map.insert("policy".into(), Value::String(p.clone()));
        }
        Value::Object(map)
    }
}

/// `adbreak.start`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AdBreakRequest {
    /// File path or URI of the clip to roll.
    pub uri: String,
    /// Programme running time to open the break on. Omit to roll now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_running_time_ms: Option<u64>,
    /// Source to rejoin afterwards. Omit to return to whatever is on air.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_to: Option<String>,
}

/// `program.golive`: add the page, add the destination, take the page.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GoLiveRequest {
    /// The page to put on air. Plain http(s); `web+` is added here.
    pub url: String,
    /// Where to send the programme. Added as an output unless one already
    /// sends there. Omit to leave the outputs alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtmp: Option<String>,
    /// "auto" (the default) or "off". See `source.add`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superimpose: Option<String>,
    /// Source id. Derived from the host when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// What `program.golive` answers with.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GoLiveResult {
    /// The source that was created or reused.
    pub source: String,
    /// The output that was created or reused, if one was asked for.
    pub output: Option<String>,
    /// Where the source is now. It goes to programme as soon as it is live.
    pub state: crate::types::SourceState,
}

/// An id on its own: `source.get`, `source.remove`, `output.remove`,
/// `output.reconnect`, `media.remove`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IdRequest {
    pub id: String,
}

/// `media.convert` and `media.remove` name a file rather than an id.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NameRequest {
    /// File name as it appears in the media listing. The REST layer puts it
    /// in the path, where the transform rule calls it `id`, so both spellings
    /// are read.
    #[serde(alias = "id")]
    pub name: String,
}

/// `program.history`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HistoryRequest {
    /// How many takes to return, newest first. At most 100.
    #[serde(default)]
    pub limit: Option<u32>,
}

/// One take, as `program.history` reports it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TakeRecord {
    /// What went on air. Null is the slate.
    pub source: Option<String>,
    /// Programme running time the cut landed on.
    pub at_running_time_ms: u64,
    /// Token id that asked for it, or "core" when the mixer did it itself.
    pub by: String,
    /// Event sequence number the take was published under.
    pub seq: u64,
}

/// What `program.get` answers with, and what `program.take` returns so that no
/// follow up read is needed.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProgramState {
    /// Source on air, or null for the slate. A scene of one full canvas item
    /// reports that item's source here too, so anything written against this
    /// before scenes existed still reads.
    pub program: Option<String>,
    /// The scene on air, when one was taken by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    /// The scene armed for the next `program.take` with no argument.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    /// Programme pipeline running time, in milliseconds.
    pub running_time_ms: u64,
    /// The previous source, which is what `program.revert` would take back to.
    pub previous: Option<String>,
    /// Present while an ad break is armed or on air.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ad: Option<crate::types::AdStatus>,
}

/// `core.subscribe`: which events, and which expensive streams.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct SubscribeRequest {
    /// Event name patterns, matched against the part after `event/`. `*`
    /// matches one or more characters: "program.*" matches `event/program.took`.
    /// An empty list subscribes to everything.
    #[serde(default)]
    pub events: Vec<String>,
    /// The expensive streams this client wants. Nothing here runs unless a
    /// client asks for it.
    #[serde(default)]
    pub ext: Ext,
}

/// The `ext` table from 03 section 6.
///
/// Every key is off by default. A terminal UI takes meters and tally and
/// declines multiview; a Stream Deck takes tally only; an agent takes nothing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Ext {
    /// The mosaic: binary frames and `event/multiview.layout`. `false` or
    /// omitted builds nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiview: Option<MultiviewExt>,
    /// `event/meters` at 10 per second.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub meters: bool,
    /// `event/tally`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tally: bool,
    /// `event/source.position` for seekable sources.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub positions: bool,
    /// The preview scene, composited in the multiview pipeline at mosaic size,
    /// or `"full"` for a full resolution preview compositor built while
    /// subscribed. See 11 section 3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<PreviewExt>,
    /// `event/telemetry`: a line of numbers per tick, at 1 to 10 per second.
    /// This is what turns the probes on; nothing measures until it is here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telemetry: Option<TelemetryExt>,
    /// `event/agent.state` when a threshold crosses or a state flips, with a
    /// snapshot URL. `true` takes the defaults from 09 section 5 item 12.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentExt>,
    /// Keys this build does not implement yet (`thumb`). Kept rather than
    /// refused so that a client written against the full table still connects,
    /// and so the core can say in the subscribe result which keys it ignored.
    #[serde(flatten, default, skip_serializing_if = "Map::is_empty")]
    pub other: Map<String, Value>,
}

/// `ext.preview`. Either `"full"`, `false`, or an object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum PreviewExt {
    /// `"full"`: a full resolution preview compositor, built while subscribed
    /// and torn down after.
    Full(String),
    /// `"preview": false`.
    Off(bool),
    /// Thumbnails at mosaic size, which is what a designer drawing handles
    /// wants and costs about 1.7 percent of a programme composite.
    On {
        #[serde(default)]
        fps: Option<u32>,
        #[serde(default)]
        width: Option<u32>,
    },
}

impl PreviewExt {
    pub fn wanted(&self) -> bool {
        match self {
            Self::Off(false) => false,
            Self::Full(s) => s == "full",
            _ => true,
        }
    }

    /// Whether this asks for the expensive full resolution compositor.
    pub fn is_full(&self) -> bool {
        matches!(self, Self::Full(s) if s == "full")
    }
}

/// `ext.telemetry`. Accepts `false` to mean off, `true` for the default rate,
/// or an object naming it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum TelemetryExt {
    /// `"telemetry": true` or `false`.
    Off(bool),
    On {
        /// Ticks per second, 1 to 10. The core clamps to that range.
        #[serde(default)]
        hz: Option<u32>,
    },
}

/// `ext.agent`. `true` takes the default thresholds; an object moves them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum AgentExt {
    /// `"agent": true` or `false`.
    On(bool),
    Thresholds {
        /// Shot change score, 0 to 1. Default 0.3.
        #[serde(default)]
        shot: Option<f64>,
        /// Fraction of the picture at black. Default 0.98.
        #[serde(default)]
        black: Option<f64>,
        /// How long the picture has to be identical. Default 200.
        #[serde(default)]
        freeze_ms: Option<u64>,
        /// How long the programme has to be quiet. Default 500.
        #[serde(default)]
        silence_ms: Option<u64>,
    },
}

/// `ext.multiview`. Accepts `false` to mean off, or an object.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MultiviewExt {
    /// `"multiview": false`.
    Off(bool),
    On {
        /// Frames per second, 1 to 30. The core clamps to what the mosaic runs at.
        #[serde(default)]
        fps: Option<u32>,
        /// Mosaic width in pixels, 320 to 1920.
        #[serde(default)]
        width: Option<u32>,
    },
}

impl MultiviewExt {
    pub fn wanted(&self) -> bool {
        !matches!(self, Self::Off(false))
    }
}

impl Ext {
    pub fn wants_multiview(&self) -> bool {
        self.multiview.as_ref().is_some_and(|m| m.wanted())
    }

    /// Whether this client wants the preview scene composited for it.
    ///
    /// The preview lives in the multiview pipeline, so asking for it is also
    /// asking for the mosaic: a client that asks for preview alone gets the
    /// mosaic built underneath it and does not have to know that.
    pub fn wants_preview(&self) -> bool {
        self.preview.as_ref().is_some_and(|p| p.wanted())
    }

    /// Whether the full resolution preview compositor was asked for.
    pub fn wants_full_preview(&self) -> bool {
        self.preview.as_ref().is_some_and(|p| p.is_full())
    }

    /// Ticks per second for `event/telemetry`, `None` when it was not asked
    /// for. Clamped to the 1 to 10 the table names.
    pub fn telemetry_hz(&self) -> Option<u32> {
        match self.telemetry.as_ref()? {
            TelemetryExt::Off(false) => None,
            TelemetryExt::Off(true) => Some(1),
            TelemetryExt::On { hz } => Some(hz.unwrap_or(1).clamp(1, 10)),
        }
    }

    /// Whether `event/agent.state` was asked for. `"agent": false` is not.
    pub fn wants_agent(&self) -> bool {
        !matches!(self.agent, None | Some(AgentExt::On(false)))
    }

    /// Keys in `ext` this build does not act on, so `core.subscribe` can say
    /// so rather than leaving a client waiting for a stream that never starts.
    pub fn unsupported(&self) -> Vec<String> {
        self.other.keys().cloned().collect()
    }
}

/// What `core.subscribe` answers with, before the snapshot arrives.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SubscribeResult {
    /// The sequence number the snapshot that follows is current as of.
    pub seq: u64,
    /// The event patterns now in force.
    pub events: Vec<String>,
    /// `ext` keys this build ignored. Empty on a build that knows them all.
    pub ignored_ext: Vec<String>,
}

/// `event/multiview.layout`: how to read the binary frames that follow.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MultiviewLayout {
    /// Stable for as long as the cells are unchanged, and carried in the
    /// header of every frame, so a client that falls behind can tell which
    /// layout a late frame belongs to.
    pub id: u32,
    pub width: i32,
    pub height: i32,
    pub cells: Vec<crate::types::CellAssignment>,
}

/// `event/tally`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Tally {
    /// Source id to "program", "preview" or "off".
    pub sources: Map<String, Value>,
}

/// `event/meters`: the programme bus and every source, in one message at 10
/// per second, rather than one message per meter as the legacy stream sends.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Meters {
    /// Peak dBFS per channel on the programme bus.
    pub program: Vec<f64>,
    /// Peak dBFS per channel, per source id.
    pub sources: Map<String, Value>,
}

/// `event/resync`: the client fell behind and the stream has a hole in it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Resync {
    /// The last sequence number the client is known to have. Everything after
    /// it was dropped; re-subscribe for a fresh snapshot.
    pub from_seq: u64,
    /// How many events were dropped.
    pub dropped: u64,
}

/// `event/flush`: the end of a batch. A client renders here and not before.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Flush {
    /// The sequence number of the last event in the batch.
    pub seq: u64,
}

/// `event/snapshot`: the full state, and where in the stream it sits.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Snapshot {
    pub seq: u64,
    pub state: Box<crate::types::MixerStatus>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A partial audio body names only what it moves. A missing `media` is an
    /// empty list rather than a list of zeroes: the difference is whether
    /// sending one fader silences every video underneath it.
    #[test]
    fn a_partial_audio_body_names_only_what_it_moves() {
        let parse = |v: Value| serde_json::from_value::<AudioRequest>(v).unwrap();
        let page_only = parse(json!({ "page": 0.8 }));
        assert_eq!(page_only.page, Some(0.8));
        assert!(page_only.media.is_empty());
        assert_eq!(page_only.gain, None);
        assert_eq!(page_only.muted, None);

        let mute = parse(json!({ "muted": true }));
        assert_eq!(mute.muted, Some(true));
        assert_eq!(mute.gain, None);
        // Unmuting is an explicit false, not an omission.
        assert_eq!(parse(json!({ "muted": false })).muted, Some(false));

        // What the UI sends when the second video's fader moves. A short list
        // could not say this: `[0.5]` would move the first video.
        let second = parse(json!({ "media": [null, 0.5] }));
        assert_eq!(second.media, vec![None, Some(0.5)]);
    }

    /// `source` wins over `scene`, an empty string is the slate, and a scene
    /// name is read as a one item scene until scenes land.
    #[test]
    fn a_take_reads_a_scene_as_a_one_item_scene() {
        let parse = |v: Value| serde_json::from_value::<TakeRequest>(v).unwrap();
        assert_eq!(parse(json!({ "source": "cam1" })).target().as_deref(), Some("cam1"));
        assert_eq!(parse(json!({ "scene": "cam2" })).target().as_deref(), Some("cam2"));
        assert_eq!(
            parse(json!({ "source": "cam1", "scene": "cam2" })).target().as_deref(),
            Some("cam1")
        );
        assert_eq!(parse(json!({})).target(), None);
        assert_eq!(parse(json!({ "source": "  " })).target(), None);
        assert_eq!(parse(json!({ "source": null })).target(), None);
    }

    /// Anything a source kind understands has to survive the trip, flat, so a
    /// plugin's `device` or `channel` key reaches its config untouched.
    #[test]
    fn unknown_source_keys_are_carried_rather_than_refused() {
        let r: AddSourceRequest =
            serde_json::from_value(json!({ "uri": "ndi://studio", "channel": 3, "id": "ndi1" }))
                .unwrap();
        assert_eq!(r.uri, "ndi://studio");
        assert_eq!(r.id.as_deref(), Some("ndi1"));
        assert_eq!(r.params["channel"], 3);
        assert!(!r.params.contains_key("uri"), "a known key must not be duplicated");
    }

    #[test]
    fn an_output_request_becomes_the_config_json() {
        let r = AddOutputRequest {
            id: "yt".into(),
            uri: "rtmp://a/b".into(),
            policy: Some("cdn".into()),
            params: [("bitrate_kbps".to_string(), json!(4500))].into_iter().collect(),
        };
        let v = r.to_config_json();
        assert_eq!(v["id"], "yt");
        assert_eq!(v["uri"], "rtmp://a/b");
        assert_eq!(v["policy"], "cdn");
        assert_eq!(v["bitrate_kbps"], 4500);
        // No policy means the config's own default, not a null that serde
        // would refuse.
        let bare = AddOutputRequest { policy: None, params: Map::new(), ..r };
        assert!(bare.to_config_json().get("policy").is_none());
    }

    /// The ext table is the contract with every client. `false` is off, an
    /// object is on, and a key this build has never heard of is remembered so
    /// the subscribe result can say it was ignored.
    #[test]
    fn the_ext_table_reads_the_shapes_the_protocol_document_names() {
        let p: SubscribeRequest = serde_json::from_value(json!({
            "events": ["program.*", "source.*", "alert"],
            "ext": { "multiview": { "fps": 8, "width": 1280 }, "meters": true, "positions": false }
        }))
        .unwrap();
        assert_eq!(p.events.len(), 3);
        assert!(p.ext.wants_multiview());
        assert!(p.ext.meters);
        assert!(!p.ext.tally);
        assert!(!p.ext.positions);
        assert!(p.ext.unsupported().is_empty());

        let off: SubscribeRequest =
            serde_json::from_value(json!({ "ext": { "multiview": false } })).unwrap();
        assert!(!off.ext.wants_multiview());

        // Telemetry and the agent push, which this build does implement.
        let ahead: SubscribeRequest = serde_json::from_value(json!({
            "ext": { "telemetry": { "hz": 4 }, "agent": true }
        }))
        .unwrap();
        assert!(ahead.ext.unsupported().is_empty());
        assert_eq!(ahead.ext.telemetry_hz(), Some(4));
        assert!(ahead.ext.wants_agent());
        // The rate is clamped to the 1 to 10 the table names, and `true` is
        // the default rate rather than an error.
        let fast: SubscribeRequest =
            serde_json::from_value(json!({ "ext": { "telemetry": { "hz": 99 } } })).unwrap();
        assert_eq!(fast.ext.telemetry_hz(), Some(10));
        let plain: SubscribeRequest =
            serde_json::from_value(json!({ "ext": { "telemetry": true } })).unwrap();
        assert_eq!(plain.ext.telemetry_hz(), Some(1));
        // And off is off, not on with a default.
        let none: SubscribeRequest =
            serde_json::from_value(json!({ "ext": { "telemetry": false, "agent": false } }))
                .unwrap();
        assert_eq!(none.ext.telemetry_hz(), None);
        assert!(!none.ext.wants_agent());

        // A client written against a key this build does not have connects,
        // and is told which keys it did nothing with.
        let ahead: SubscribeRequest =
            serde_json::from_value(json!({ "ext": { "thumb": { "fps": 2 } } })).unwrap();
        assert_eq!(ahead.ext.unsupported(), vec!["thumb".to_string()]);

        // Nothing at all is the agent's subscription: no expensive stream runs.
        let bare: SubscribeRequest = serde_json::from_value(json!({})).unwrap();
        assert!(!bare.ext.wants_multiview());
        assert!(!bare.ext.meters && !bare.ext.tally && !bare.ext.positions);
        assert_eq!(bare.ext.telemetry_hz(), None);
        assert!(!bare.ext.wants_agent());
    }
}
