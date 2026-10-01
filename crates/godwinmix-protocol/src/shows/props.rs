//! The properties wave 4 gave a show: whether it composites, what it takes
//! in when it does not, and how healthy it is. See
//! `dev/plans/wave4-contract.md`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// What a show without compositing takes in. A show that composites makes
/// its input its one source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InputSpec {
    /// `udp://@239.1.1.1:5000`, `srt://...`, `rtmp://host/app/key`,
    /// `rtsp://...`, `https://.../x.m3u8`, `file:///clip.ts`, `rist://...`,
    /// or a channel's stream, `channel:<app>/<stream>`.
    pub uri: String,
    /// The MPEG-TS program of a feed that carries several. Left out: the
    /// first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<u16>,
    /// Per transport: `interface` for multicast, `latency` for SRT,
    /// `passphrase`. Passed to the host as given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Map<String, Value>>,
    /// Switched to when the input stalls, and back when it returns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup: Option<Box<InputSpec>>,
}

/// How a show is, in one word.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum HealthState {
    Ok,
    Warning,
    Alarm,
    /// Stopped, or not running yet.
    #[default]
    Off,
}

/// What an alarm is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AlarmKind {
    NoInput,
    Stall,
    Black,
    Freeze,
    Silence,
    CcErrors,
    Loss,
    OutputFailed,
    GovernorRefused,
    Shed,
}

impl AlarmKind {
    /// An alarm means a viewer is not getting the show; the rest are
    /// warnings.
    pub fn is_alarm(self) -> bool {
        matches!(
            self,
            AlarmKind::NoInput | AlarmKind::Stall | AlarmKind::Black | AlarmKind::Freeze | AlarmKind::OutputFailed | AlarmKind::GovernorRefused
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Alarm {
    pub kind: AlarmKind,
    /// When it began, in unix milliseconds.
    pub since_ms: u64,
    /// What a person reads.
    #[serde(default)]
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Health {
    pub state: HealthState,
    #[serde(default)]
    pub alarms: Vec<Alarm>,
}

impl Health {
    /// The state a list of alarms comes to.
    pub fn of(alarms: Vec<Alarm>) -> Health {
        let state = if alarms.iter().any(|a| a.kind.is_alarm()) {
            HealthState::Alarm
        } else if !alarms.is_empty() {
            HealthState::Warning
        } else {
            HealthState::Ok
        };
        Health { state, alarms }
    }

    /// Whether `other` says something different: its state or its set of
    /// alarm kinds, never a number alone.
    pub fn differs(&self, other: &Health) -> bool {
        let kinds = |h: &Health| {
            let mut k: Vec<AlarmKind> = h.alarms.iter().map(|a| a.kind).collect();
            k.sort();
            k
        };
        self.state != other.state || kinds(self) != kinds(other)
    }
}

/// What the input is doing, as the host last counted it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InputStats {
    #[serde(default)]
    pub kbps: u32,
    #[serde(default)]
    pub fps: f64,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_codec: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_codec: Option<String>,
    #[serde(default)]
    pub audio_channels: u32,
    #[serde(default)]
    pub cc_errors: u64,
    #[serde(default)]
    pub packets_lost: u64,
    /// Between the last two keyframes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyframe_ms: Option<u64>,
    /// Since the last frame arrived.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_frame_ms: Option<u64>,
}

/// What one output is doing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OutputStats {
    pub id: String,
    /// waiting, connecting, live, reconnecting, failed, or off.
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub kbps: u32,
    #[serde(default)]
    pub reconnects: u32,
    /// `copy`, or what the plan gave it, such as `h264 1280x720`.
    #[serde(default)]
    pub rendition_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoder: Option<String>,
    #[serde(default)]
    pub cpu_millicores: u32,
}

/// One show's numbers, as `show.stats` answers them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ShowStats {
    pub id: String,
    pub health: Health,
    /// None for a show with no input, or before the host has counted any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<InputStats>,
    #[serde(default)]
    pub outputs: Vec<OutputStats>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alarm(kind: AlarmKind) -> Alarm {
        Alarm { kind, since_ms: 1, detail: String::new() }
    }

    #[test]
    fn alarms_come_to_a_state_and_only_a_change_of_kind_counts() {
        assert_eq!(Health::of(vec![]).state, HealthState::Ok);
        assert_eq!(Health::of(vec![alarm(AlarmKind::Silence)]).state, HealthState::Warning);
        let black = Health::of(vec![alarm(AlarmKind::Black), alarm(AlarmKind::Silence)]);
        assert_eq!(black.state, HealthState::Alarm);
        let later = Health::of(vec![Alarm { since_ms: 99, ..alarm(AlarmKind::Silence) }, alarm(AlarmKind::Black)]);
        assert!(!black.differs(&later), "the same kinds in another order, at another time");
        assert!(black.differs(&Health::of(vec![alarm(AlarmKind::Black)])));
        assert_eq!(serde_json::to_value(AlarmKind::CcErrors).unwrap(), "cc-errors");
    }
}
