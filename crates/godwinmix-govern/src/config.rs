//! The governor's settings. There is nothing a person has to set: every
//! field has a default that means "work it out" (Decision 2).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `[governor]` in the station's config.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct GovernorConfig {
    /// Advanced. Cores to keep free for something else on this machine, in
    /// place of the reserve the governor works out. Off when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reserve_cores: Option<f32>,
    /// The uplink, in kbit/s, when it is known (measured by an output, or
    /// set under Advanced). Copies are counted against it; without it the
    /// governor does not refuse on bandwidth.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uplink_kbps: Option<u32>,
    /// How many sessions the calibration opens on a hardware encoder before
    /// it calls the device unlimited. 8 when absent, which finds a consumer
    /// NVIDIA card's limit; a card with the limit lifted (a headend's) sets
    /// it higher, up to 256, to find the real one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nvenc_session_probe: Option<u32>,
    /// The number of encoder sessions an NVIDIA device takes, in place of
    /// what calibration found. For a driver whose limit is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nvenc_sessions: Option<u32>,
    /// The page runs on this machine too (the desktop app). Set by the
    /// station from how it was started, never by a person, so it is not
    /// read from a file.
    #[serde(skip)]
    pub desktop: bool,
}

/// The most sessions a probe opens, whatever is asked: each is a small
/// live encode, held until the probe ends.
pub const MAX_SESSION_PROBE: u32 = 256;

/// Sessions a probe opens when nothing is set.
pub const SESSION_PROBE: u32 = 8;

impl GovernorConfig {
    /// Sessions the calibration opens before calling a device unlimited.
    pub fn session_probe(&self) -> u32 {
        self.nvenc_session_probe.unwrap_or(SESSION_PROBE).clamp(1, MAX_SESSION_PROBE)
    }

    /// The session limit of `device` set by hand, which wins over the probe.
    /// It is NVIDIA's (`nvidia`, as the catalogue names its acceleration).
    pub fn sessions_on(&self, device: &str) -> Option<u32> {
        self.nvenc_sessions.filter(|_| device == "nvidia")
    }

    /// The override in thousandths of a core, when there is one.
    pub fn reserve_override(&self) -> Option<u32> {
        self.reserve_cores.filter(|c| c.is_finite() && *c >= 0.0).map(|c| (c * 1000.0).round() as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_table_means_work_it_out() {
        let c: GovernorConfig = toml_like("{}");
        assert_eq!(c, GovernorConfig::default());
        assert_eq!(c.reserve_override(), None);
    }

    #[test]
    fn the_override_is_read_in_cores() {
        let c: GovernorConfig = toml_like(r#"{"reserve_cores": 1.5}"#);
        assert_eq!(c.reserve_override(), Some(1500));
        let bad = GovernorConfig { reserve_cores: Some(-1.0), ..Default::default() };
        assert_eq!(bad.reserve_override(), None);
    }

    #[test]
    fn the_probe_goes_as_high_as_asked_and_a_set_limit_is_nvidia_s() {
        assert_eq!(GovernorConfig::default().session_probe(), 8);
        let c: GovernorConfig = toml_like(r#"{"nvenc_session_probe": 128, "nvenc_sessions": 64}"#);
        assert_eq!(c.session_probe(), 128);
        assert_eq!(c.sessions_on("nvidia"), Some(64));
        assert_eq!(c.sessions_on("videotoolbox"), None);
        assert_eq!(GovernorConfig { nvenc_session_probe: Some(100_000), ..Default::default() }.session_probe(), MAX_SESSION_PROBE);
    }

    fn toml_like(json: &str) -> GovernorConfig {
        serde_json::from_str(json).unwrap()
    }
}
