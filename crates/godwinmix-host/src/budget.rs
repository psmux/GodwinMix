//! `[plugins.<name>]` budgets, and what happens when one is broken.
//!
//! A plugin that eats the machine is the failure mode every extension system
//! has. The core will not police a plugin's code, but it can watch what the
//! process costs and act on one instance alone, never on itself:
//!
//! ```toml
//! [plugins.ndi]
//! max_rss_mb = 512
//! max_cpu_percent = 60
//! on_over_budget = "restart"   # or "disable", or "alert"
//! ```
//!
//! On a breach the core logs, emits `event/plugin.state {state: "over-budget"}`
//! and does what the policy says. A breach has to hold for a few samples
//! before it counts, because one second of a plugin opening a file is not a
//! plugin that is broken.

use serde::{Deserialize, Serialize};

/// What to do about an instance that is over its budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OverBudget {
    /// Stop and start that instance. The freeze frame covers the gap.
    Restart,
    /// Stop it and leave it stopped, so the show carries on without it.
    Disable,
    /// Say so and do nothing. The default, because a programme that keeps
    /// running is the safe state.
    #[default]
    Alert,
}

impl OverBudget {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Restart => "restart",
            Self::Disable => "disable",
            Self::Alert => "alert",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "restart" => Some(Self::Restart),
            "disable" => Some(Self::Disable),
            "alert" => Some(Self::Alert),
            _ => None,
        }
    }
}

/// How many consecutive samples over the limit count as a breach. At one
/// sample a second that is three seconds of sustained overuse.
pub const BREACH_SAMPLES: u32 = 3;

/// One plugin's limits, from `[plugins.<name>]`. Absent means no limit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Budget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_rss_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cpu_percent: Option<f64>,
    #[serde(default)]
    pub on_over_budget: OverBudget,
}

impl Budget {
    pub fn is_set(&self) -> bool {
        self.max_rss_mb.is_some() || self.max_cpu_percent.is_some()
    }

    /// Read the three keys out of a `[plugins.<name>]` table, ignoring
    /// everything else in it: the rest is the plugin's own settings and the
    /// core does not read inside.
    pub fn from_table(table: &impl toml_like::Table) -> Self {
        Self {
            max_rss_mb: table.integer("max_rss_mb").map(|v| v.max(0) as u64),
            max_cpu_percent: table.float("max_cpu_percent"),
            on_over_budget: table
                .string("on_over_budget")
                .and_then(|s| OverBudget::parse(&s))
                .unwrap_or_default(),
        }
    }

    /// What is over, if anything, as a sentence naming the number and the
    /// limit.
    pub fn breach(&self, stats: &Stats) -> Option<String> {
        if let (Some(limit), Some(rss)) = (self.max_rss_mb, stats.rss_bytes) {
            let mb = rss / (1024 * 1024);
            if mb > limit {
                return Some(format!("{mb} MB resident, over the {limit} MB budget"));
            }
        }
        if let (Some(limit), Some(cpu)) = (self.max_cpu_percent, stats.cpu_percent) {
            if cpu > limit {
                return Some(format!("{cpu:.0} percent of a core, over the {limit:.0} budget"));
            }
        }
        None
    }
}

/// The numbers `plugin.list` and `plugin.stats` carry, per instance.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_percent: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rss_bytes: Option<u64>,
    /// What the plugin said its own delay is, or what the core measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_latency_ms: Option<u32>,
    pub buffers_dropped: u64,
    pub restarts: u32,
}

/// Counts consecutive breaches so one busy second is not a policy decision.
#[derive(Debug, Clone, Default)]
pub struct Watch {
    over: u32,
}

impl Watch {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one sample. `Some(reason)` when the breach has held long enough to
    /// act on, and the count resets so the same breach is acted on once.
    pub fn observe(&mut self, budget: &Budget, stats: &Stats) -> Option<String> {
        match budget.breach(stats) {
            None => {
                self.over = 0;
                None
            }
            Some(reason) => {
                self.over += 1;
                if self.over >= BREACH_SAMPLES {
                    self.over = 0;
                    Some(reason)
                } else {
                    None
                }
            }
        }
    }
}

/// The two accessors `Budget::from_table` needs, so this crate reads a TOML
/// table without depending on a TOML parser of its own.
///
/// The core hands in its `config::Params`, which is `toml::Table`; a test
/// hands in the tiny map below. Either way the budget reader is the same
/// three keys.
pub mod toml_like {
    /// Anything that can answer "what is at this key".
    pub trait Table {
        fn integer(&self, key: &str) -> Option<i64>;
        fn float(&self, key: &str) -> Option<f64>;
        fn string(&self, key: &str) -> Option<String>;
    }

    /// A map of key to value, for a test and for anything that has already
    /// turned its configuration into JSON.
    impl Table for serde_json::Map<String, serde_json::Value> {
        fn integer(&self, key: &str) -> Option<i64> {
            self.get(key).and_then(serde_json::Value::as_i64)
        }
        fn float(&self, key: &str) -> Option<f64> {
            self.get(key).and_then(serde_json::Value::as_f64)
        }
        fn string(&self, key: &str) -> Option<String> {
            self.get(key).and_then(serde_json::Value::as_str).map(str::to_string)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn stats(rss_mb: u64, cpu: f64) -> Stats {
        Stats {
            rss_bytes: Some(rss_mb * 1024 * 1024),
            cpu_percent: Some(cpu),
            ..Default::default()
        }
    }

    #[test]
    fn a_budget_reads_its_three_keys_and_ignores_the_plugins_own_settings() {
        let table = json!({
            "max_rss_mb": 512,
            "max_cpu_percent": 60.0,
            "on_over_budget": "restart",
            "discovery_interval_secs": 30
        });
        let budget = Budget::from_table(table.as_object().expect("a table"));
        assert_eq!(budget.max_rss_mb, Some(512));
        assert_eq!(budget.max_cpu_percent, Some(60.0));
        assert_eq!(budget.on_over_budget, OverBudget::Restart);
    }

    #[test]
    fn a_plugin_with_no_budget_is_never_over_it() {
        let budget = Budget::default();
        assert!(!budget.is_set());
        assert_eq!(budget.breach(&stats(4096, 400.0)), None);
        assert_eq!(budget.on_over_budget, OverBudget::Alert, "doing nothing is the default");
    }

    #[test]
    fn a_breach_names_the_number_and_the_limit() {
        let budget = Budget { max_rss_mb: Some(256), ..Default::default() };
        let reason = budget.breach(&stats(512, 1.0)).expect("512 is over 256");
        assert!(reason.contains("512"), "{reason}");
        assert!(reason.contains("256"), "{reason}");
    }

    #[test]
    fn one_busy_second_is_not_a_policy_decision() {
        let budget = Budget { max_cpu_percent: Some(50.0), ..Default::default() };
        let mut watch = Watch::new();
        assert_eq!(watch.observe(&budget, &stats(1, 90.0)), None, "the first sample waits");
        assert_eq!(watch.observe(&budget, &stats(1, 90.0)), None, "so does the second");
        assert!(watch.observe(&budget, &stats(1, 90.0)).is_some(), "the third acts");
        assert_eq!(watch.observe(&budget, &stats(1, 90.0)), None, "and the count starts again");
        // Coming back under the limit clears it outright.
        watch.observe(&budget, &stats(1, 90.0));
        watch.observe(&budget, &stats(1, 10.0));
        assert_eq!(watch.observe(&budget, &stats(1, 90.0)), None);
    }
}
