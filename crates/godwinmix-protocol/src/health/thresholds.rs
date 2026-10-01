//! What counts as black, frozen, silent or stalled, per show.
//!
//! Every field has a default a headend operator can live with, and a
//! duration of zero switches that one check off. The station keeps these with
//! the show and hands them to whoever measures (the direct host in the
//! `monitor` field of a direct table row, a compositing show in its own
//! `[vitals]` section), so a person sets them once, for one show, from the
//! page.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Thresholds for one show's alarms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Thresholds {
    /// Seconds a picture must stay black before `black` is raised. 0: off.
    pub black_secs: f64,
    /// An 8 bit luma at or under which a pixel counts as black. 38 is ten
    /// percent of the way from video black (16) to white (235), the figure
    /// ffmpeg's blackdetect uses.
    pub black_luma: u8,
    /// The share of pixels that must be black for the picture to be.
    pub black_ratio: f64,
    /// Seconds a picture must stay unchanged before `freeze` is raised. 0: off.
    pub freeze_secs: f64,
    /// The mean luma difference between two samples, 0 to 1, under which the
    /// picture counts as unchanged.
    pub freeze_diff: f64,
    /// Seconds the sound must stay quiet before `silence` is raised. 0: off.
    pub silence_secs: f64,
    /// The peak level in dBFS under which the sound counts as quiet.
    pub silence_db: f64,
    /// Seconds without a single packet of input before `stall` is raised.
    pub stall_secs: f64,
    /// Continuity errors within `window_secs` that raise `cc-errors`. 0: off.
    pub cc_errors: u64,
    /// Packets lost within `window_secs` that raise `loss`. 0: off.
    pub loss: u64,
    /// The window the two counters are judged over.
    pub window_secs: f64,
}

impl Default for Thresholds {
    fn default() -> Thresholds {
        Thresholds {
            black_secs: 4.0,
            black_luma: 38,
            black_ratio: 0.98,
            freeze_secs: 10.0,
            freeze_diff: 0.002,
            silence_secs: 10.0,
            silence_db: -60.0,
            stall_secs: 3.0,
            cc_errors: 5,
            loss: 20,
            window_secs: 10.0,
        }
    }
}

/// A condition that has to hold for a while before it counts: black for four
/// seconds, quiet for ten.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Hold {
    since_ms: Option<u64>,
}

impl Hold {
    /// Say whether the condition holds at `now_ms`. The first true starts the
    /// clock; a false stops it.
    pub fn set(&mut self, holds: bool, now_ms: u64) {
        match (holds, self.since_ms) {
            (true, None) => self.since_ms = Some(now_ms),
            (false, _) => self.since_ms = None,
            _ => {}
        }
    }

    /// When the condition began, if it has held for `secs` by `now_ms`.
    /// Never for a `secs` of zero, which is the check switched off.
    pub fn fired(&self, now_ms: u64, secs: f64) -> Option<u64> {
        let since = self.since_ms?;
        let due = (secs * 1000.0) as u64;
        (secs > 0.0 && now_ms.saturating_sub(since) >= due).then_some(since)
    }

    pub fn clear(&mut self) {
        self.since_ms = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hold_fires_after_its_time_and_a_break_starts_it_again() {
        let mut h = Hold::default();
        h.set(true, 1_000);
        assert_eq!(h.fired(4_999, 4.0), None);
        assert_eq!(h.fired(5_000, 4.0), Some(1_000));
        h.set(true, 6_000);
        assert_eq!(h.fired(6_000, 4.0), Some(1_000), "the clock keeps its start");
        h.set(false, 7_000);
        h.set(true, 8_000);
        assert_eq!(h.fired(11_000, 4.0), None);
        assert_eq!(h.fired(99_000, 0.0), None, "zero seconds is off");
    }

    #[test]
    fn a_partial_set_of_thresholds_takes_the_rest_from_the_defaults() {
        let t: Thresholds = serde_json::from_value(serde_json::json!({"black_secs": 2})).unwrap();
        assert_eq!(t.black_secs, 2.0);
        assert_eq!(t.freeze_secs, Thresholds::default().freeze_secs);
    }
}
