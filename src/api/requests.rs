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
    /// Id of the source to put on air. Null or omitted cuts to the slate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The scene to take, once scenes exist (11). Today a scene name is read
    /// as a one item scene, which is to say as a source id, and `source` wins
    /// when both are given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    /// Programme running time to land the cut on, in milliseconds. Omit for
    /// immediate. Read the current running time from `core.info` or a status
    /// snapshot first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_running_time_ms: Option<u64>,
}

impl TakeRequest {
    /// What actually goes on air: `source` when given, otherwise the scene
    /// name read as a one item scene.
    pub fn target(&self) -> Option<String> {
        self.source
            .clone()
            .or_else(|| self.scene.clone())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
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
    pub state: crate::api::types::SourceState,
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
    /// File name as it appears in the media listing.
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
    /// Source on air, or null for the slate.
    pub program: Option<String>,
    /// Programme pipeline running time, in milliseconds.
    pub running_time_ms: u64,
    /// The previous source, which is what `program.revert` would take back to.
    pub previous: Option<String>,
    /// Present while an ad break is armed or on air.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ad: Option<crate::api::types::AdStatus>,
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
    /// Keys this build does not implement yet (`thumb`, `preview`,
    /// `telemetry`, `agent`). Kept rather than refused so that a client
    /// written against the full table still connects, and so the core can say
    /// in the subscribe result which keys it ignored.
    #[serde(flatten, default, skip_serializing_if = "Map::is_empty")]
    pub other: Map<String, Value>,
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
    pub cells: Vec<crate::api::types::CellAssignment>,
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
    pub state: Box<crate::api::types::MixerStatus>,
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

        // A client written against the whole table connects, and is told which
        // keys this build did nothing with.
        let ahead: SubscribeRequest = serde_json::from_value(json!({
            "ext": { "telemetry": { "hz": 4 }, "agent": true }
        }))
        .unwrap();
        let mut ignored = ahead.ext.unsupported();
        ignored.sort();
        assert_eq!(ignored, vec!["agent".to_string(), "telemetry".to_string()]);

        // Nothing at all is the agent's subscription: no expensive stream runs.
        let bare: SubscribeRequest = serde_json::from_value(json!({})).unwrap();
        assert!(!bare.ext.wants_multiview());
        assert!(!bare.ext.meters && !bare.ext.tally && !bare.ext.positions);
    }
}
