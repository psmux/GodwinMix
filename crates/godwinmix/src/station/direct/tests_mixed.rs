//! A show that composites, linked to a real station state the way its
//! process would be: what it says of its health is kept, announced and read
//! back, and a link that closes reads as an alarm, not as the last word.

use crate::station::host::Linked;
use crate::station::link::serve::Host;
use crate::station::link::Hello;
use crate::station::registry::{Record, Registry};
use crate::station::state::{Launch, Station};
use godwinmix_core::mixer::MixerHandle;
use godwinmix_govern::{Governor, GovernorConfig, Profile};
use godwinmix_protocol::shows::{Alarm, AlarmKind, AlarmSettings, Health, HealthState, ShowState};
use godwinmix_protocol::types::Event;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::broadcast::Receiver;

type Heard = Receiver<godwinmix_core::state::Envelope>;

fn station(name: &str) -> (Arc<Station>, Heard) {
    gstreamer::init().unwrap();
    let dir = std::env::temp_dir().join(format!("gmx-direct-mixed-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut reg = Registry::open(&dir.join("godwinmix.toml")).unwrap();
    reg.records.push(Record::new("mixed", "Mixed", None));
    let (events, _commands) = MixerHandle::detached();
    let heard = events.subscribe();
    let governor = Governor::with_machine(GovernorConfig::default(), Profile::uncalibrated(), 8, 16_384);
    let render = godwinmix_core::render::Station::with_governor(governor, &godwinmix_core::catalogue::global(), Default::default());
    let launch = Launch { exe: "none".into(), common: Vec::new(), calibration: None };
    let tokens = Arc::new(godwinmix_protocol::scope::Tokens::new(Vec::new(), false));
    (Station::new(reg, events, tokens, render, launch), heard)
}

/// What the supervisor would have set before the process said hello.
fn supervised(st: &Station, pid: u32) {
    let mut procs = st.procs.lock();
    let p = procs.entry("mixed".into()).or_default();
    p.state = ShowState::Starting;
    p.pid = Some(pid);
    p.secret = "s".into();
    p.stop = Some(tokio::sync::watch::channel(false).0);
}

fn hello(pid: u32) -> Hello {
    Hello { show: "mixed".into(), addr: "127.0.0.1:9".parse().unwrap(), secret: "s".into(), pid }
}

fn heard_health(heard: &mut Heard) -> Vec<(HealthState, Vec<AlarmKind>)> {
    let mut out = Vec::new();
    while let Ok(e) = heard.try_recv() {
        if let Event::ShowHealth { id, health } = e.event {
            assert_eq!(id, "mixed");
            out.push((health.state, health.alarms.iter().map(|a| a.kind).collect()));
        }
    }
    out
}

#[tokio::test]
async fn a_mixed_show_s_health_crosses_the_link_and_a_lost_link_reads_as_a_stall() {
    let (st, mut heard) = station("link");
    let host = Linked(st.clone());
    supervised(&st, 7);
    st.direct.starting("mixed");
    assert_eq!(st.direct.health_of(&st, "mixed").state, HealthState::Off, "starting and not linked yet");
    assert!(host.hello(&hello(7)));
    assert_eq!(st.direct.health_of(&st, "mixed").state, HealthState::Ok);

    let black = Alarm { kind: AlarmKind::Black, since_ms: 1_000, detail: "black".into() };
    host.health("mixed", Health::from_alarms(vec![black.clone()]));
    host.health("mixed", Health::from_alarms(vec![Alarm { detail: "still black".into(), ..black }]));
    let stats = st.direct.stats_of(&st, "mixed");
    assert_eq!(stats.health.state, HealthState::Alarm);
    assert_eq!(stats.health.alarms[0].kind, AlarmKind::Black);
    assert_eq!(st.view("mixed").unwrap().health.alarms[0].kind, AlarmKind::Black, "show.list carries it too");

    // Killed: the link closes while the supervisor still holds it.
    host.gone("mixed", 7);
    let gone = st.direct.health_of(&st, "mixed");
    assert_eq!((gone.state, gone.alarms[0].kind), (HealthState::Alarm, AlarmKind::Stall), "not the black it last sent: {gone:?}");

    // Started again: a new process links, and what the old one said is past.
    supervised(&st, 8);
    assert_eq!(st.direct.health_of(&st, "mixed").alarms[0].kind, AlarmKind::Stall, "still down while it starts again");
    assert!(host.hello(&hello(8)));
    assert_eq!(st.direct.health_of(&st, "mixed").state, HealthState::Ok);

    // The first word on the show, then black, then the stall, then ok again;
    // the second black changed only its detail and sent nothing.
    let sent = heard_health(&mut heard);
    let want = vec![
        (HealthState::Ok, vec![]),
        (HealthState::Alarm, vec![AlarmKind::Black]),
        (HealthState::Alarm, vec![AlarmKind::Stall]),
        (HealthState::Ok, vec![]),
    ];
    assert_eq!(sent, want);
}

#[tokio::test]
async fn a_show_stopped_on_purpose_reads_as_off_without_an_alarm_on_the_way() {
    let (st, mut heard) = station("stopped");
    let host = Linked(st.clone());
    supervised(&st, 3);
    assert!(host.hello(&hello(3)));
    // `supervise::stop` takes the handle before it stops the process.
    st.procs.lock().get_mut("mixed").unwrap().stop = None;
    host.gone("mixed", 3);
    st.procs.lock().get_mut("mixed").unwrap().state = ShowState::Stopped;
    assert_eq!(st.direct.health_of(&st, "mixed").state, HealthState::Off);
    let sent = heard_health(&mut heard);
    assert!(sent.iter().all(|(s, _)| *s != HealthState::Alarm), "{sent:?}");
}

#[test]
fn the_wall_s_alarm_settings_become_what_vitals_set_takes() {
    let set = AlarmSettings { enabled: Some(true), black_ms: Some(2500), freeze_ms: None, silence_ms: Some(0), silence_dbfs: Some(-50.0) };
    let v = super::vitals_settings(&set);
    assert_eq!(v, json!({"alarms": true, "black_secs": 2.5, "silence_secs": 0.0, "silence_db": -50.0}));
    let parsed: godwinmix_core::vitals::VitalsConfig = serde_json::from_value(v).unwrap();
    assert!(parsed.alarms);
    assert_eq!((parsed.thresholds.black_secs, parsed.thresholds.silence_db), (2.5, -50.0));
    assert_eq!(super::vitals_settings(&AlarmSettings::default()), json!({"alarms": false}), "off unless asked, for a show that composites");
}
