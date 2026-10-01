//! A show's health: the host's alarms, and what only the station knows
//! (an output the governor would not admit, a show process that failed),
//! as one state. Announced as `event/show.health` when the state or the
//! set of alarm kinds moves, and never for a number.

use super::Direct;
use crate::station::state::Station;
use godwinmix_protocol::destination::DestinationState;
use godwinmix_protocol::shows::{Alarm, AlarmKind, Health, HealthState, ShowState};
use godwinmix_protocol::types::Event;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

impl Direct {
    /// What a show's health is now, each alarm dated from when it began.
    pub fn health_of(&self, st: &Station, id: &str) -> Health {
        let judged = self.judge(st, id);
        self.pin(id, judged)
    }

    /// Keep each alarm's `since_ms` where it was first seen until the alarm
    /// clears. An alarm the station adds itself is judged afresh on every
    /// read, and a window alarm from the host slides with its window, so
    /// without this the wall would show every alarm's age as nothing.
    fn pin(&self, id: &str, mut health: Health) -> Health {
        let mut seen = self.seen.lock();
        if health.alarms.is_empty() {
            if let Some(s) = seen.get_mut(id) {
                s.alarm_since.clear();
            }
            return health;
        }
        let s = seen.entry(id.to_string()).or_default();
        s.alarm_since.retain(|kind, _| health.alarms.iter().any(|a| a.kind == *kind));
        for a in &mut health.alarms {
            a.since_ms = *s.alarm_since.entry(a.kind).or_insert(a.since_ms);
        }
        health.alarms.sort_by_key(|a| (a.since_ms, a.kind));
        health
    }

    fn judge(&self, st: &Station, id: &str) -> Health {
        let Some(r) = st.registry.lock().get(id).cloned() else { return Health::off() };
        if r.stopped {
            return Health::off();
        }
        let seen = self.seen.lock().get(id).cloned().unwrap_or_default();
        let since = |kind| seen.host_health.as_ref().and_then(|h| h.alarms.iter().find(|a| a.kind == kind)).map(|a| a.since_ms);
        let mut alarms: Vec<Alarm> = seen.host_health.as_ref().map(|h| h.alarms.clone()).unwrap_or_default();
        let mut add = |kind: AlarmKind, detail: String| {
            if !alarms.iter().any(|a| a.kind == kind) {
                alarms.push(Alarm { kind, since_ms: since(kind).unwrap_or_else(now_ms), detail });
            }
        };
        if r.compositing {
            match st.state_of(id) {
                Some(ShowState::Failed) => return Health { state: HealthState::Alarm, alarms },
                Some(ShowState::Stopped) | None => return Health::off(),
                _ if r.input.is_none() => return Health::from_alarms(alarms),
                _ => {}
            }
        }
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
