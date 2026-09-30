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
    /// The page runs on this machine too (the desktop app). Set by the
    /// station from how it was started, never by a person, so it is not
    /// read from a file.
    #[serde(skip)]
    pub desktop: bool,
}

impl GovernorConfig {
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

    fn toml_like(json: &str) -> GovernorConfig {
        serde_json::from_str(json).unwrap()
    }
}
