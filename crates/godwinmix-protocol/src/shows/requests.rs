//! The params and answers of the wave 4 show methods: `show.set`,
//! `show.output.*` and `show.stats`.

use super::props::InputSpec;
use super::stats::ShowStats;
use crate::rendition::RenditionChoice;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

/// An output as it is given to a show without compositing: an address, or
/// a platform and a key.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ShowOutputSpec {
    /// A slug, unique within the show. Made from the label when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// youtube, facebook, twitch, custom or srt. Left out: custom, which
    /// takes a whole address in `uri`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The whole address: `srt://10.0.0.9:9000`, `rtmp://host/app/key`,
    /// `udp://239.2.2.2:5000`, or `hls://viewers` for HLS served from the
    /// station's own port. For a platform, its ingest server when it is not
    /// the platform's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    /// A platform's stream key. Write only: no method reads it back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// On by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Left out: a copy of the input's own bytes, repackaged. Otherwise a
    /// rendition request or `{"preset": "youtube-720p30"}`, planned and
    /// admitted by the governor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rendition: Option<RenditionChoice>,
    /// For an `hls://` output: segment and part lengths, the window and the
    /// viewer key, as an `hls/output` takes them. Refused on any other.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<HlsOutputParams>,
}

/// The `params` of a show's `hls://` output: the same names, defaults and
/// limits as an `hls/output`'s (`docs/reference/hls-output.md`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HlsOutputParams {
    /// Target segment length, 500 to 10000. Segments are cut at the first
    /// keyframe at or after it. Default 2000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment_ms: Option<u32>,
    /// LL-HLS part length; 0 is plain HLS. Default 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_ms: Option<u32>,
    /// true gives parts of 333 ms when `part_ms` names no other length.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub low_latency: Option<bool>,
    /// Seconds of the past each rung keeps and lists. Default 30.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<u32>,
    /// The key a viewer's link carries, at least 16 characters. Left out:
    /// derived from the show and the output on this machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewer_key: Option<String>,
}

/// `show.output.add`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ShowOutputAddRequest {
    /// The show. `show` is taken as another name for it.
    #[serde(alias = "show")]
    pub id: String,
    /// The new output's own id, a slug. Made from the label when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    /// Write only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rendition: Option<RenditionChoice>,
    /// For an `hls://` output, as in `show.add`'s outputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<HlsOutputParams>,
}

impl ShowOutputAddRequest {
    /// The output it asks for, as `show.add` takes one.
    pub fn spec(&self) -> ShowOutputSpec {
        ShowOutputSpec {
            id: self.output.clone(),
            platform: self.platform.clone(),
            label: self.label.clone(),
            uri: self.uri.clone(),
            key: self.key.clone(),
            enabled: self.enabled,
            rendition: self.rendition.clone(),
            params: self.params.clone(),
        }
    }
}

/// `show.output.set`. Names only what moves.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ShowOutputSetRequest {
    /// The show. `show` is taken as another name for it.
    #[serde(alias = "show")]
    pub id: String,
    /// The output's id.
    pub output: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    /// A new key. Left out keeps the one it has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Left out keeps what it has; `null` or `{"preset": "copy"}` goes back
    /// to a copy.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<RenditionChoice>")]
    pub rendition: Option<Option<RenditionChoice>>,
    /// An `hls://` output's params, all of them: a name left out goes back
    /// to its default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<HlsOutputParams>,
}

fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<RenditionChoice>>, D::Error> {
    Option::<RenditionChoice>::deserialize(d).map(Some)
}

/// `show.output.remove`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShowOutputRemoveRequest {
    /// The show. `show` is taken as another name for it.
    #[serde(alias = "show")]
    pub id: String,
    pub output: String,
}

/// `show.set`. Names only what moves.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ShowSetRequest {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// true starts a show process whose one source is the input and moves
    /// the outputs to it; false goes back to a show without compositing,
    /// when it has one source and no scenes in use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compositing: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<InputSpec>,
    /// Alarm settings; the fields named move, the rest stay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alarms: Option<super::props::AlarmSettings>,
}

/// What a switch of compositing did, in `show.set`'s answer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SwitchReport {
    /// What the show does now.
    pub compositing: bool,
    /// The outputs that moved.
    pub outputs: Vec<String>,
    /// From the moment the outputs stopped where they were to the moment
    /// every one of them was live again where they went. None when they
    /// were not all live within the wait, or there were none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap_ms: Option<u64>,
    /// What a person should know: an output that was still connecting when
    /// the answer was sent, and so on.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// `show.set`'s answer: the show, and what a switch of compositing did.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShowSetResult {
    #[serde(flatten)]
    pub show: super::Show,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub switch: Option<SwitchReport>,
}

/// `show.stats`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ShowStatsRequest {
    /// Left out: every show.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<String>>,
    /// Of `health`, `input` and `outputs`. Left out: all three.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<String>>,
}

/// `show.stats`'s answer.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ShowStatsList {
    pub shows: Vec<ShowStats>,
}
