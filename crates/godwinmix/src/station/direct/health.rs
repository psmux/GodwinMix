//! A show's health: the host's alarms, and what only the station knows
//! (an output the governor would not admit, a show process that failed),
//! as one state. Announced as `event/show.health` when the state or the
//! set of alarm kinds moves, and never for a number.

use super::Direct;
use crate::station::state::Station;
use godwinmix_protocol::destination::DestinationState;
use godwinmix_protocol::shows::{Alarm, AlarmKind, Health, ShowState};
use godwinmix_protocol::types::Event;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

impl Direct {
    /// What a show's health is now.
    pub fn health_of(&self, st: &Station, id: &str) -> Health {
        let Some(r) = st.registry.lock().get(id).cloned() else { return Health::off() };
        if r.stopped {
            return Health::off();
        }
        let seen = self.seen.lock().get(id).cloned().unwrap_or_default();
        let since = |kind| seen.host_health.as_ref().and_then(|h| h.alarms.iter().find(|a| a.kind == kind)).map(|a| a.since_ms);
        let mut alarms: Vec<Alarm> = seen.host_health.as_ref().map(|h| h.alarms.clone()).unwrap_or_default();
        if r.compositing {
            if let Some(settled) = process_health(st.state_of(id), seen.lost_ms) {
                return settled;
            }
            for a in seen.show_health.iter().flat_map(|h| h.alarms.iter()) {
                if !alarms.iter().any(|b| b.kind == a.kind) {
                    alarms.push(a.clone());
                }
            }
            if r.input.is_none() {
                return Health::from_alarms(alarms);
            }
        }
        let mut add = |kind: AlarmKind, detail: String| {
            if !alarms.iter().any(|a| a.kind == kind) {
                alarms.push(Alarm { kind, since_ms: since(kind).unwrap_or_else(now_ms), detail });
            }
        };
        if r.input.is_some() && !self.plugins().is_some_and(|p| p.is_running(crate::channels::PLUGIN)) {
            add(AlarmKind::NoInput, "the ingest plugin, which runs shows without compositing, is not running".into());
        } else if r.input.is_some() && !seen.input_live() {
            add(AlarmKind::NoInput, "nothing has arrived on the input yet".into());
        }
        for o in r.outputs.iter().filter(|o| o.enabled && !r.compositing) {
            if let (_, Some(no)) = self.transcode.view(id, &o.id) {
                add(AlarmKind::GovernorRefused, format!("{}: {}", o.id, no.message));
            }
            if seen.output(&o.id, true).state == DestinationState::Failed {
                add(AlarmKind::OutputFailed, format!("{} failed", o.id));
            }
        }
        Health::from_alarms(alarms)
    }

    /// Send `event/show.health` when the show's health moved.
    pub fn announce_health(&self, st: &Station, id: &str) {
        if st.registry.lock().get(id).is_none() {
            return;
        }
        let now = self.health_of(st, id);
        let moved = {
            let mut seen = self.seen.lock();
            let s = seen.entry(id.to_string()).or_default();
            let moved = s.announced.as_ref().is_none_or(|was| was.changed_from(&now));
            if moved {
                s.announced = Some(now.clone());
            }
            moved
        };
        if moved {
            st.events.emit(Event::ShowHealth { id: id.to_string(), health: now });
        }
    }
}

/// The health of a show that composites when its process decides it alone:
/// stopped or not yet linked is `off`, and failed or lost is an alarm, never
/// the last health the show sent. None while it runs and is linked.
fn process_health(state: Option<ShowState>, lost_ms: Option<u64>) -> Option<Health> {
    let stall = |since_ms, detail: &str| Health::from_alarms(vec![Alarm { kind: AlarmKind::Stall, since_ms, detail: detail.into() }]);
    match (state, lost_ms) {
        (Some(ShowState::Stopped) | None, _) => Some(Health::off()),
        // Started on purpose and not linked yet: nothing to judge.
        (Some(ShowState::Starting), None) => Some(Health::off()),
        (Some(ShowState::Failed), lost) => Some(stall(
            lost.unwrap_or_else(now_ms),
            "the show's process kept dying and the station stopped starting it; start it again once the cause is fixed",
        )),
        (_, Some(lost)) => Some(stall(
            lost,
            "the show's process ended or lost its link to the station; it stays in alarm until a process of the show is running and has looked at its programme again",
        )),
        (Some(ShowState::Running), None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use godwinmix_protocol::shows::HealthState;

    #[test]
    fn a_show_process_that_failed_or_went_reads_as_a_stall_and_one_starting_reads_as_off() {
        assert_eq!(process_health(Some(ShowState::Stopped), Some(5)), Some(Health::off()));
        assert_eq!(process_health(Some(ShowState::Starting), None), Some(Health::off()));
        assert_eq!(process_health(Some(ShowState::Running), None), None);
        for state in [ShowState::Starting, ShowState::Running, ShowState::Failed] {
            let h = process_health(Some(state), Some(42)).unwrap();
            assert_eq!(h.state, HealthState::Alarm, "{state:?}");
            assert_eq!((h.alarms[0].kind, h.alarms[0].since_ms), (AlarmKind::Stall, 42));
        }
        assert_eq!(process_health(Some(ShowState::Failed), None).unwrap().alarms[0].kind, AlarmKind::Stall);
    }
}
