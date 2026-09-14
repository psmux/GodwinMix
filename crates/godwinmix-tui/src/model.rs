//! The slice of the control protocol this UI reads.
//!
//! Every type here mirrors a `$defs` entry in `protocol.json` and nothing
//! else. They are deliberately forgiving: every field has a default and
//! unknown fields are ignored, so a core one API level ahead still paints and
//! a core that leaves an optional field out does not take the screen down.
//! The protocol says `additionalProperties: true` on the status records, and
//! this is the client side of that promise.

use serde::Deserialize;
use std::collections::BTreeMap;

/// `MixerStatus`: the whole state, as `event/snapshot` carries it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MixerStatus {
    #[serde(default)]
    pub sources: Vec<SourceStatus>,
    #[serde(default)]
    pub outputs: Vec<OutputStatus>,
    #[serde(default)]
    pub multiview: MultiviewStatus,
    #[serde(default)]
    pub uptime_secs: u64,
    #[serde(default)]
    pub running_time_ms: u64,
    #[serde(default)]
    pub backend: BackendInfo,
    #[serde(default)]
    pub program: Option<String>,
    #[serde(default)]
    pub ad: Option<AdStatus>,
}

/// `SourceStatus`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SourceStatus {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub uri: String,
    #[serde(default)]
    pub state: SourceState,
    #[serde(default)]
    pub has_video: bool,
    #[serde(default)]
    pub has_audio: bool,
    #[serde(default = "unity")]
    pub gain: f64,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub seekable: bool,
    #[serde(default)]
    pub position_ms: Option<u64>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub cell: Option<u32>,
    /// Not in api_level 1. A core that grows `source.set {color}` will send it
    /// and the tile takes that colour instead of the one derived from the id.
    #[serde(default)]
    pub color: Option<String>,
}

fn unity() -> f64 {
    1.0
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceState {
    #[default]
    Connecting,
    Live,
    Stalled,
    Failed,
}

impl SourceState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::Live => "live",
            Self::Stalled => "stalled",
            Self::Failed => "failed",
        }
    }
}

/// `OutputStatus`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OutputStatus {
    pub id: String,
    #[serde(default)]
    pub uri_host: String,
    #[serde(default)]
    pub state: OutputState,
    #[serde(default)]
    pub reconnects: u32,
    #[serde(default)]
    pub queue_secs: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputState {
    #[default]
    Connecting,
    Live,
    Reconnecting,
    Failed,
}

impl OutputState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::Live => "live",
            Self::Reconnecting => "reconnecting",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MultiviewStatus {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
    #[serde(default)]
    pub cols: u32,
    #[serde(default)]
    pub rows: u32,
    #[serde(default)]
    pub fps: i32,
    #[serde(default)]
    pub cells: Vec<CellAssignment>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CellAssignment {
    #[serde(default)]
    pub index: u32,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub w: i32,
    #[serde(default)]
    pub h: i32,
}

/// `event/multiview.layout`: how to read the binary frames that follow.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MultiviewLayout {
    #[serde(default)]
    pub id: u32,
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
    #[serde(default)]
    pub cells: Vec<CellAssignment>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BackendInfo {
    #[serde(default)]
    pub video_decoder: String,
    #[serde(default)]
    pub video_encoder: String,
    #[serde(default)]
    pub audio_decoder: String,
    #[serde(default)]
    pub audio_encoder: String,
    #[serde(default)]
    pub hardware_accelerated: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AdStatus {
    #[serde(default)]
    pub uri: String,
    #[serde(default)]
    pub on_air: bool,
    #[serde(default)]
    pub return_to: Option<String>,
}

/// `event/meters`: peak dBFS per channel, for the programme bus and every
/// source, ten times a second.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Meters {
    #[serde(default)]
    pub program: Vec<f64>,
    #[serde(default)]
    pub sources: BTreeMap<String, Vec<f64>>,
}

impl Meters {
    /// The loudest channel, which is what a one line meter shows.
    pub fn peak(levels: &[f64]) -> Option<f64> {
        levels.iter().copied().fold(None, |acc: Option<f64>, v| Some(acc.map_or(v, |a| a.max(v))))
    }
}

/// One `event/alert`, with the time it was seen.
#[derive(Debug, Clone)]
pub struct Alert {
    pub severity: String,
    pub message: String,
    /// Wall clock UTC, as `HH:MM:SS`.
    pub at: String,
}

/// An error answer from the core, kept whole because the message is written to
/// be read: it names the current state and the next step.
#[derive(Debug, Clone, Default)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    /// `data.retryable`, when the core sent one.
    pub retryable: Option<bool>,
}

/// A linear fader (0.0 to 10.0) as decibels. Silence reads as the floor
/// rather than as minus infinity, so a step up from silence lands somewhere.
pub const GAIN_FLOOR_DB: f64 = -60.0;

pub fn gain_to_db(gain: f64) -> f64 {
    if gain <= 0.0 {
        GAIN_FLOOR_DB
    } else {
        (20.0 * gain.log10()).max(GAIN_FLOOR_DB)
    }
}

pub fn db_to_gain(db: f64) -> f64 {
    if db <= GAIN_FLOOR_DB {
        0.0
    } else {
        (10f64.powf(db / 20.0)).clamp(0.0, 10.0)
    }
}

/// One decibel up or down from wherever the fader is now.
pub fn step_gain(gain: f64, steps: f64) -> f64 {
    let db = (gain_to_db(gain) + steps).clamp(GAIN_FLOOR_DB, 20.0);
    let out = db_to_gain(db);
    (out * 1000.0).round() / 1000.0
}

/// `hh:mm:ss` from a count of milliseconds, which is how the programme running
/// time and a clip's position are both shown.
pub fn clock_ms(ms: u64) -> String {
    let total = ms / 1000;
    format!("{:02}:{:02}:{:02}", total / 3600, (total / 60) % 60, total % 60)
}

/// `hh:mm:ss` from a count of seconds.
pub fn clock_secs(secs: u64) -> String {
    clock_ms(secs.saturating_mul(1000))
}

/// The time of day in UTC, without pulling in a date library for it. The
/// alerts list wants a timestamp an operator can match against a log line and
/// nothing more than that.
pub fn utc_hms(unix_secs: u64) -> String {
    let s = unix_secs % 86_400;
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}
