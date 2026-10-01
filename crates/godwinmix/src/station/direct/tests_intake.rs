//! The host's events, played by the test, against a real station state:
//! what is kept, what show.stats reads back, and when event/show.health is
//! sent. No plugin runs, so the station knows the host is not there.

use crate::station::direct::take_event;
use crate::station::registry::{Record, Registry};
use crate::station::state::{Launch, Station};
use godwinmix_core::mixer::MixerHandle;
use godwinmix_govern::{Governor, GovernorConfig, Profile};
use godwinmix_protocol::shows::{AlarmKind, HealthState, InputSpec};
use godwinmix_protocol::types::Event;
use serde_json::json;
use std::sync::Arc;

fn station(name: &str) -> (Arc<Station>, tokio::sync::broadcast::Receiver<godwinmix_core::state::Envelope>) {
    gstreamer::init().unwrap();
    let dir = std::env::temp_dir().join(format!("gmx-direct-intake-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut reg = Registry::open(&dir.join("godwinmix.toml")).unwrap();
    let mut feed = Record::new("feed", "Feed", None);
    feed.compositing = false;
    feed.input = Some(InputSpec { uri: "udp://@239.1.1.1:5000".into(), program: None, params: None, backup: None });
    reg.records.push(feed);
    let (events, _commands) = MixerHandle::detached();
    let heard = events.subscribe();
    let governor = Governor::with_machine(GovernorConfig::default(), Profile::uncalibrated(), 8, 16_384);
    let render = godwinmix_core::render::Station::with_governor(governor, &godwinmix_core::catalogue::global(), Default::default());
    let launch = Launch { exe: "none".into(), common: Vec::new(), calibration: None };
    let tokens = Arc::new(godwinmix_protocol::scope::Tokens::new(Vec::new(), false));
    (Station::new(reg, events, tokens, render, launch), heard)
}

fn health_events(heard: &mut tokio::sync::broadcast::Receiver<godwinmix_core::state::Envelope>) -> Vec<HealthState> {
    let mut out = Vec::new();
    while let Ok(e) = heard.try_recv() {
        if let Event::ShowHealth { health, .. } = e.event {
            out.push(health.state);
        }
    }
    out
}

#[test]
fn what_the_host_says_is_kept_read_back_and_announced_only_when_health_moves() {
    let (st, mut heard) = station("kept");
    let health = st.direct.health_of(&st, "feed");
    assert_eq!(health.state, HealthState::Alarm);
    assert_eq!(health.alarms[0].kind, AlarmKind::NoInput, "no host runs: {health:?}");

    take_event(&st, "direct.input", &json!({"show": "feed", "state": "live", "since_ms": 1, "relay": "127.0.0.1:1", "stream": "~feed/main",
        "video": {"codec": "h264", "width": 1920, "height": 1080, "fps": 25.0, "kbps": 6000}}));
    take_event(&st, "direct.health", &json!({"show": "feed", "health": {"state": "warning", "alarms": [{"kind": "silence", "since_ms": 5, "detail": "no sound"}]}}));
    take_event(&st, "direct.health", &json!({"show": "feed", "health": {"state": "warning", "alarms": [{"kind": "silence", "since_ms": 5, "detail": "still none"}]}}));
    take_event(&st, "direct.stats", &json!({"shows": [{"id": "feed", "input": {"kbps": 6100, "fps": 25.0, "cc_errors": 3}, "outputs": []},
                                                      {"id": "stranger", "input": {"kbps": 1}}]}));
    take_event(&st, "direct.output", &json!({"show": "feed", "output": "nope", "state": "live", "since_ms": 0, "kbps": 0, "reconnects": 0, "error": null}));

    let stats = st.direct.stats_of(&st, "feed");
    assert_eq!(stats.input.as_ref().map(|i| (i.kbps, i.cc_errors)), Some((6100, 3)));
    assert!(st.direct.seen.lock().get("stranger").is_none(), "a show the station does not have is not kept");
    assert!(st.direct.seen.lock().get("feed").unwrap().outputs.is_empty(), "an output the show does not have is not kept");
    // The host's own alarm, and no input alarm, since no plugin runs: the
    // station adds that one itself.
    let kinds: Vec<AlarmKind> = stats.health.alarms.iter().map(|a| a.kind).collect();
    assert!(kinds.contains(&AlarmKind::Silence) && kinds.contains(&AlarmKind::NoInput), "{kinds:?}");
    let sent = health_events(&mut heard);
    // The first word on the show (no input), then silence joining it; the
    // second silence report changed only its detail and sent nothing.
    assert_eq!(sent, vec![HealthState::Alarm, HealthState::Alarm], "{sent:?}");
}

#[test]
fn a_stopped_show_is_off_and_a_show_list_view_carries_the_new_properties() {
    let (st, _heard) = station("view");
    let view = st.view("feed").unwrap();
    assert!(!view.compositing);
    assert_eq!(view.state, godwinmix_protocol::shows::ShowState::Running);
    assert_eq!(view.input.as_ref().map(|i| i.uri.as_str()), Some("udp://@239.1.1.1:5000"));
    st.registry.lock().get_mut("feed").unwrap().stopped = true;
    assert_eq!(st.view("feed").unwrap().health.state, HealthState::Off);
    assert_eq!(st.view("feed").unwrap().state, godwinmix_protocol::shows::ShowState::Stopped);
    let table = st.direct.table(&st);
    assert_eq!(table, json!([]), "a stopped show has no row");
    st.registry.lock().get_mut("feed").unwrap().stopped = false;
    let table = st.direct.table(&st);
    assert_eq!(table[0]["id"], "feed");
    assert_eq!(table[0]["monitor"]["alarms"], true);
    assert!(toml::Value::try_from(&table).is_ok(), "{table}");
}
