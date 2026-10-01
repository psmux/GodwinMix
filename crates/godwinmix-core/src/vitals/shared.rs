//! What the running vitals read every second, and what they last judged:
//! the one place `vitals.get` and `vitals.set` reach.

use std::sync::{Arc, OnceLock};

use godwinmix_protocol::health::{Health, Thresholds};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// `[vitals]`, and what `vitals.set` changes: the thresholds, and whether
/// to keep a mosaic up for the picture alarms while nobody is looking.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct VitalsConfig {
    pub alarms: bool,
    #[serde(flatten)]
    pub thresholds: Thresholds,
}

#[derive(Debug, Default)]
pub struct Shared {
    cfg: RwLock<VitalsConfig>,
    health: RwLock<Option<Health>>,
}

impl Shared {
    pub fn new(cfg: VitalsConfig) -> Arc<Shared> {
        Arc::new(Shared { cfg: RwLock::new(cfg), health: RwLock::default() })
    }

    pub fn settings(&self) -> VitalsConfig {
        self.cfg.read().clone()
    }

    /// New settings, read by the vitals at their next look, within a second.
    pub fn set(&self, cfg: VitalsConfig) {
        *self.cfg.write() = cfg;
    }

    /// The health last judged; `None` before the first second has passed.
    pub fn health(&self) -> Option<Health> {
        self.health.read().clone()
    }

    pub(super) fn judged(&self, health: Health) {
        *self.health.write() = Some(health);
    }
}

static PROCESS: OnceLock<Arc<Shared>> = OnceLock::new();

/// This process's vitals: one show, one programme.
pub fn process() -> Arc<Shared> {
    PROCESS.get_or_init(|| Shared::new(VitalsConfig::default())).clone()
}

/// Set at start, from the config file's `[vitals]`, which the core's own
/// `Config` keeps among the tables it does not know. Not a table these
/// fields fit: the defaults.
pub fn configure(extra: &std::collections::BTreeMap<String, toml::Value>) {
    process().set(from_extra(extra));
}

fn from_extra(extra: &std::collections::BTreeMap<String, toml::Value>) -> VitalsConfig {
    extra.get("vitals").cloned().and_then(|v| v.try_into().ok()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_section_is_read_flat_and_the_rest_are_defaults() {
        let cfg: crate::config::Config = toml::from_str("[vitals]\nalarms = true\nblack_secs = 2\nsilence_db = -50\n").unwrap();
        let v = from_extra(&cfg.extra);
        assert!(v.alarms);
        assert_eq!((v.thresholds.black_secs, v.thresholds.silence_db), (2.0, -50.0));
        assert_eq!(v.thresholds.freeze_secs, Thresholds::default().freeze_secs);
        assert_eq!(from_extra(&Default::default()), VitalsConfig::default());
    }

    #[test]
    fn a_setting_is_read_back_and_health_waits_for_the_first_look() {
        let shared = Shared::new(VitalsConfig::default());
        assert!(shared.health().is_none());
        let cfg: VitalsConfig = serde_json::from_value(serde_json::json!({"alarms": true, "freeze_secs": 0})).unwrap();
        shared.set(cfg.clone());
        assert_eq!(shared.settings(), cfg);
        shared.judged(Health::default());
        assert!(shared.health().is_some());
    }
}
