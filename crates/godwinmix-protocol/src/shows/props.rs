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
    pub backup: Option<BackupInput>,
}

/// An input's backup: an input with no backup of its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BackupInput {
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Map<String, Value>>,
}

/// A show's alarms, as a person sets them from the page. Left out fields
/// keep the measuring side's defaults; a duration of 0 switches that check
/// off.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AlarmSettings {
    /// Whether black, freeze and silence are watched at all. Left out: on
    /// for a show without compositing, off for one that composites.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub black_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freeze_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub silence_ms: Option<u64>,
    /// The peak level under which sound counts as quiet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub silence_dbfs: Option<f64>,
}

impl AlarmSettings {
    /// The `monitor.thresholds` the measuring side reads
    /// (`crate::health::Thresholds`), with only the fields a person set.
    pub fn thresholds(&self) -> serde_json::Value {
        let mut t = Map::new();
        let secs = |ms: u64| Value::from(ms as f64 / 1000.0);
        if let Some(ms) = self.black_ms {
            t.insert("black_secs".into(), secs(ms));
        }
        if let Some(ms) = self.freeze_ms {
            t.insert("freeze_secs".into(), secs(ms));
        }
        if let Some(ms) = self.silence_ms {
            t.insert("silence_secs".into(), secs(ms));
        }
        if let Some(db) = self.silence_dbfs {
            t.insert("silence_db".into(), Value::from(db));
        }
        Value::Object(t)
    }

    /// Lay `other` over these: what it names moves, the rest stays.
    pub fn merged(&self, other: &AlarmSettings) -> AlarmSettings {
        AlarmSettings {
            enabled: other.enabled.or(self.enabled),
            black_ms: other.black_ms.or(self.black_ms),
            freeze_ms: other.freeze_ms.or(self.freeze_ms),
            silence_ms: other.silence_ms.or(self.silence_ms),
            silence_dbfs: other.silence_dbfs.or(self.silence_dbfs),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alarm_settings_become_the_thresholds_the_host_reads_and_merge_field_by_field() {
        let set = AlarmSettings { black_ms: Some(2500), silence_dbfs: Some(-50.0), ..Default::default() };
        assert_eq!(set.thresholds(), serde_json::json!({"black_secs": 2.5, "silence_db": -50.0}));
        let t: crate::health::Thresholds = serde_json::from_value(set.thresholds()).unwrap();
        assert_eq!(t.black_secs, 2.5);
        assert_eq!(t.freeze_secs, crate::health::Thresholds::default().freeze_secs);
        let later = set.merged(&AlarmSettings { enabled: Some(false), black_ms: Some(0), ..Default::default() });
        assert_eq!((later.enabled, later.black_ms, later.silence_dbfs), (Some(false), Some(0), Some(-50.0)));
    }
}
