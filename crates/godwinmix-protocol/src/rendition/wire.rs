//! The rendition and governor methods on the wire: what `output.add` takes as
//! `rendition`, what `rendition.presets`, `rendition.plan` and
//! `governor.status` answer, the two events, and the `data` of a refusal.
//! `dev/plans/wave2-contract.md` is where these shapes were agreed.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Cost, RenditionRequest};

/// What an output asks for: a whole request, or a preset by id.
///
/// A request's `id` is replaced by the output's own id (a ladder's rungs get
/// `<output>-<rung>`), so a client may send any slug there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum RenditionChoice {
    /// `{"preset": "youtube-1080p30"}`.
    Preset(PresetRef),
    /// `{"ladder": [...]}`: a custom ABR ladder, top rung first, for an
    /// `hls/output`. Each rung's `id` names it (`<output>-<id>`).
    Ladder(LadderRef),
    Request(RenditionRequest),
}

/// A custom ladder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LadderRef {
    pub ladder: Vec<RenditionRequest>,
}

/// A preset named by id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresetRef {
    pub preset: String,
}

/// One built in preset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RenditionPreset {
    /// `youtube-1080p30`, `abr-ladder-4`.
    pub id: String,
    /// What the page shows: "YouTube 1080p30".
    pub title: String,
    /// `platform`, `ladder`, `audio` or `copy`, for grouping in a menu.
    pub group: String,
    /// The one rendition, or the top rung of a ladder.
    pub request: RenditionRequest,
    /// Every rung, top first, for a ladder preset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ladder: Option<Vec<RenditionRequest>>,
    /// What the whole preset would cost here (every rung, the scaling and
    /// the sound), as the governor prices it on this machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<Cost>,
    /// Whether this machine can make it.
    #[serde(default = "yes")]
    pub available: bool,
    /// Why not, when it cannot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

fn yes() -> bool {
    true
}

/// `rendition.presets`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct PresetsResult {
    pub presets: Vec<RenditionPreset>,
}

/// `rendition.plan`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct PlanRequest {
    /// `programme` (the default) or `channel:<id>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// Why the planner decided what it did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanReason {
    /// `hardware`, `software-only`, `device-full`, `shape-unsupported`,
    /// `copied`, `transcoded`.
    pub code: String,
    pub text: String,
}

/// One node of a plan, as the page draws it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanNode {
    /// Stable across plans: `encode:programme:h264:1280x720p30:2800k:g2000`.
    pub id: String,
    /// `source`, `copy`, `decode`, `scale`, `encode`, `audio-convert`,
    /// `audio-encode`, `mux`.
    pub kind: String,
    /// The request ids (output ids, or `<output>-<rung>`) it works for.
    pub serves: Vec<String>,
    /// The catalogue id of the encoder, on an encode node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<PlanReason>,
    pub cost: Cost,
    /// Set while the governor has this node stopped to keep what is on air.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shed: Option<String>,
}

/// Use of one hardware device by a plan.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeviceTotal {
    pub millis: u32,
    pub sessions: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanTotals {
    pub cpu_millicores: u32,
    pub devices: BTreeMap<String, DeviceTotal>,
    pub egress_kbps: u32,
}

/// `rendition.plan`, and the `plan` of `event/rendition.plan`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanView {
    pub nodes: Vec<PlanNode>,
    pub totals: PlanTotals,
}

/// `event/rendition.plan`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RenditionPlanEvent {
    pub scope: String,
    pub plan: PlanView,
}

/// One thing the governor stopped or slowed, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ShedNote {
    /// "the 360p30 H.264 rendition for hls-main".
    pub what: String,
    /// The alert text.
    pub why: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CpuUse {
    pub cores: u32,
    pub used_millicores: u32,
    pub room_millicores: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeviceUse {
    pub id: String,
    /// `videotoolbox`, `nvidia`, `va`.
    pub kind: String,
    pub used_millis: u32,
    pub room_millis: u32,
    pub sessions_used: u32,
    /// Absent when the device showed no limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sessions_max: Option<u32>,
}

/// `governor.status`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GovernorStatus {
    /// Unix seconds of the calibration in use; absent before the first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calibrated_at: Option<u64>,
    /// The key the calibration is stored under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// True while a calibration is running.
    #[serde(default)]
    pub calibrating: bool,
    pub cpu: CpuUse,
    pub devices: Vec<DeviceUse>,
    pub egress_kbps: u32,
    pub shed: Vec<ShedNote>,
}

/// `governor.calibrate`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct CalibrateRequest {
    /// Measure even though something is on air. The measurement takes a few
    /// seconds of every core and can cost what is on air frames.
    #[serde(default)]
    pub confirm: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct CalibrateResult {
    pub started: bool,
}

/// One thing a refused rendition could be instead, as a button.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RenditionAdvice {
    /// "720p30 H.264 on h264-videotoolbox fits".
    pub text: String,
    /// Send this as the output's `rendition` to take the advice.
    pub request: RenditionRequest,
}

/// The `data` of a Safety refusal from the governor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RenditionRefusal {
    pub need: Cost,
    pub have: Cost,
    pub advice: Vec<RenditionAdvice>,
}
