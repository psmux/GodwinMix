//! A show's health: the host's alarms, and what only the station knows
//! (an output the governor would not admit, one that keeps trying to
//! connect with the input live, a show process that failed),
//! as one state. Announced as `event/show.health` when the state or the
//! set of alarm kinds moves, and never for a number.

use super::Direct;
use crate::station::state::Station;
use godwinmix_protocol::destination::{DestinationLive, DestinationState};
use godwinmix_protocol::shows::{Alarm, AlarmKind, Health, ShowState};
use godwinmix_protocol::types::Event;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::warn;

/// How long an output may connect or reconnect, with the input live,
/// before the show is in alarm for it.
const STUCK_MS: u64 = 60_000;

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
            let hls = self.hls.view(id, &o.id).map(|(live, _)| live);
            let live = super::outputs::unreadable(id, &o.id).or(hls).unwrap_or_else(|| seen.output(&o.id, true));
            if let Some(why) = not_sending(&o.id, &live, seen.input_live()) {
                add(AlarmKind::OutputFailed, why);
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
                // An output that fails says so in the log once, with why:
                // one stuck trying wrote nothing after its first warning.
                let before = |a: &Alarm| s.announced.as_ref().is_some_and(|h| h.alarms.iter().any(|b| b.kind == a.kind));
                for a in now.alarms.iter().filter(|a| a.kind == AlarmKind::OutputFailed && !before(a)) {
                    warn!(show = id, "{}", a.detail);
                }
                s.announced = Some(now.clone());
            }
            moved
        };
        if moved {
            st.events.emit(Event::ShowHealth { id: id.to_string(), health: now });
        }
    }
}

/// Why an output sends nothing, when it is an alarm: it failed, or it has
/// tried to connect for `STUCK_MS` with the input live. Trying again and
/// again is sending nothing too: an HLS output that never once read a
/// frame off its rendition said `ok` for as long as it tried.
fn not_sending(id: &str, live: &DestinationLive, input_live: bool) -> Option<String> {
    match live.state {
        DestinationState::Failed => Some(format!("{id} failed")),
        DestinationState::Connecting | DestinationState::Reconnecting if input_live && live.since_ms >= STUCK_MS => {
            let why = live.error.as_deref().unwrap_or("it has not connected");
            Some(format!("{id} has sent nothing for {} s: {why}", live.since_ms / 1000))
        }
        _ => None,
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
    fn an_output_trying_to_connect_for_a_minute_with_the_input_live_is_an_alarm() {
        let trying = |state, secs: u64| DestinationLive { state, since_ms: secs * 1000, error: Some("nothing came from the relay for 30 s".into()), ..Default::default() };
        assert_eq!(not_sending("hls", &trying(DestinationState::Reconnecting, 30), true), None, "not yet");
        let stuck = not_sending("hls", &trying(DestinationState::Reconnecting, 61), true).expect("a minute of trying is an alarm");
        assert_eq!(stuck, "hls has sent nothing for 61 s: nothing came from the relay for 30 s");
        assert!(not_sending("hls", &trying(DestinationState::Connecting, 90), true).is_some());
        assert_eq!(not_sending("hls", &trying(DestinationState::Reconnecting, 90), false), None, "the input is down, and says so itself");
        assert_eq!(not_sending("hls", &trying(DestinationState::Live, 90), true), None);
        assert_eq!(not_sending("hls", &trying(DestinationState::Failed, 0), false).as_deref(), Some("hls failed"));
    }

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
