//! A show's health: one state and the alarms behind it.
//!
//! The direct host reports it for every show it runs (`event/direct.health`),
//! a show that composites reports its own (`event/health`), and the station
//! hands both to clients as `event/show.health {id, health}`
//! (`dev/plans/wave4-contract.md`, Monitoring). Each is sent when the state or
//! the set of alarm kinds changes, never for a number alone; the numbers are
//! read with `show.stats`. `docs/reference/show-health.md` says what each
//! alarm watches, its threshold and what it costs.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod thresholds;
pub use thresholds::{Hold, Thresholds};

/// The one word a monitoring wall colours a row by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum HealthState {
    /// Running and nothing is wrong.
    #[default]
    Ok,
    /// Something is degraded but the picture and sound are going out:
    /// continuity errors or packet loss on the input.
    Warning,
    /// Something a viewer notices, or nothing is going out: no input, a
    /// stall, black, a frozen picture, silence, a failed output, a rendition
    /// the governor refused or shed.
    Alarm,
    /// Not monitored: the show is stopped or has no input to watch.
    Off,
}

/// What an alarm is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
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
    /// Whether this kind makes the show `warning` rather than `alarm`.
    pub fn is_warning(self) -> bool {
        matches!(self, AlarmKind::CcErrors | AlarmKind::Loss)
    }

    /// The kind as it is spelled on the wire, `cc-errors`.
    pub fn as_str(self) -> &'static str {
        match self {
            AlarmKind::NoInput => "no-input",
            AlarmKind::Stall => "stall",
            AlarmKind::Black => "black",
            AlarmKind::Freeze => "freeze",
            AlarmKind::Silence => "silence",
            AlarmKind::CcErrors => "cc-errors",
            AlarmKind::Loss => "loss",
            AlarmKind::OutputFailed => "output-failed",
            AlarmKind::GovernorRefused => "governor-refused",
            AlarmKind::Shed => "shed",
        }
    }
}

/// One condition that holds now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Alarm {
    pub kind: AlarmKind,
    /// Unix milliseconds when the condition began. For black, freeze and
    /// silence that is when the picture or sound first measured so, not when
    /// the alarm's duration ran out.
    pub since_ms: u64,
    /// One sentence for a person: what was measured, and against what.
    pub detail: String,
}

/// A show's health.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Health {
    pub state: HealthState,
    /// Every alarm that holds now, oldest first.
    pub alarms: Vec<Alarm>,
}

impl Health {
    /// The health a set of alarms adds up to: `alarm` if any is an alarm,
    /// `warning` if all are warnings, `ok` if there are none.
    pub fn from_alarms(mut alarms: Vec<Alarm>) -> Health {
        alarms.sort_by_key(|a| (a.since_ms, a.kind));
        let state = if alarms.is_empty() {
            HealthState::Ok
        } else if alarms.iter().all(|a| a.kind.is_warning()) {
            HealthState::Warning
        } else {
            HealthState::Alarm
        };
        Health { state, alarms }
    }

    /// A show nobody is monitoring.
    pub fn off() -> Health {
        Health { state: HealthState::Off, alarms: Vec::new() }
    }

    /// Whether `other` differs in a way worth an event: the state, or the
    /// set of alarm kinds. A detail whose number moved is not a change.
    pub fn changed_from(&self, other: &Health) -> bool {
        let kinds = |h: &Health| {
            let mut k: Vec<AlarmKind> = h.alarms.iter().map(|a| a.kind).collect();
            k.sort();
            k
        };
        self.state != other.state || kinds(self) != kinds(other)
    }
}

/// `event/health`, from a show that composites, about itself. The station
/// sends it on to clients as `event/show.health` with the show's id.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HealthEvent {
    pub health: Health,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alarm(kind: AlarmKind, since_ms: u64) -> Alarm {
        Alarm { kind, since_ms, detail: String::new() }
    }

    #[test]
    fn the_state_is_the_worst_alarm() {
        assert_eq!(Health::from_alarms(vec![]).state, HealthState::Ok);
        assert_eq!(Health::from_alarms(vec![alarm(AlarmKind::Loss, 1)]).state, HealthState::Warning);
        let both = Health::from_alarms(vec![alarm(AlarmKind::Black, 5), alarm(AlarmKind::CcErrors, 2)]);
        assert_eq!(both.state, HealthState::Alarm);
        assert_eq!(both.alarms[0].kind, AlarmKind::CcErrors, "oldest first");
    }

    #[test]
    fn a_moved_number_is_not_a_change_and_a_new_kind_is() {
        let a = Health::from_alarms(vec![Alarm { kind: AlarmKind::Loss, since_ms: 1, detail: "3 lost".into() }]);
        let b = Health::from_alarms(vec![Alarm { kind: AlarmKind::Loss, since_ms: 1, detail: "9 lost".into() }]);
        assert!(!a.changed_from(&b));
        let c = Health::from_alarms(vec![alarm(AlarmKind::Loss, 1), alarm(AlarmKind::Black, 2)]);
        assert!(c.changed_from(&a));
    }

    #[test]
    fn kinds_are_spelled_as_the_contract_spells_them() {
        let v = serde_json::to_value(alarm(AlarmKind::CcErrors, 0)).unwrap();
        assert_eq!(v["kind"], "cc-errors");
        assert_eq!(AlarmKind::OutputFailed.as_str(), "output-failed");
        let v = serde_json::to_value(AlarmKind::NoInput).unwrap();
        assert_eq!(v, AlarmKind::NoInput.as_str());
    }
}
