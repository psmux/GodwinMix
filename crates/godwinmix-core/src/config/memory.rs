//! `[memory]`: the most a show may hold before its memory guard acts.
//!
//! See `mixer::memguard` for what the guard does and
//! `docs/reference/configuration.md` for the operator's view.

use serde::{Deserialize, Serialize};

const MIB: u64 = 1024 * 1024;

/// The ceiling the automatic threshold never goes past: 4 GB.
pub const AUTO_CEILING: u64 = 4096 * MIB;

#[derive(Debug, Clone, Default, Deserialize, Serialize, schemars::JsonSchema)]
pub struct MemoryConfig {
    /// Megabytes of private memory past which the show raises a `memory`
    /// alarm, names its largest queues in the log and restarts the source
    /// holding the most. Unset is a quarter of the machine's memory or 4096,
    /// whichever is lower. 0 switches the guard off.
    #[serde(default)]
    pub guard_mb: Option<u64>,
}

impl MemoryConfig {
    /// The threshold in bytes on a machine with `physical` bytes of memory,
    /// or `None` when the guard is switched off.
    pub fn threshold(&self, physical: Option<u64>) -> Option<u64> {
        match self.guard_mb {
            Some(0) => None,
            Some(mb) => Some(mb.saturating_mul(MIB)),
            None => Some(physical.map_or(AUTO_CEILING, |p| (p / 4).min(AUTO_CEILING))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * MIB;

    #[test]
    fn unset_is_a_quarter_of_the_machine_up_to_four_gigabytes() {
        let auto = MemoryConfig::default();
        assert_eq!(auto.threshold(Some(8 * GB)), Some(2 * GB));
        assert_eq!(auto.threshold(Some(32 * GB)), Some(4 * GB));
        assert_eq!(auto.threshold(None), Some(4 * GB), "a machine that cannot say gets the ceiling");
    }

    #[test]
    fn a_number_is_megabytes_and_zero_is_off() {
        let set: MemoryConfig = toml::from_str("guard_mb = 1500").unwrap();
        assert_eq!(set.threshold(Some(64 * GB)), Some(1500 * MIB));
        let off: MemoryConfig = toml::from_str("guard_mb = 0").unwrap();
        assert_eq!(off.threshold(Some(64 * GB)), None);
    }
}
