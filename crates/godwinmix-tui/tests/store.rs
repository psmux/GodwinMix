//! The store, on its own: a snapshot, then deltas, then a flush.

mod support;

use godwinmix_tui::model::SourceState;
use godwinmix_tui::store::{Outcome, Store};
use serde_json::json;
use support::example_status;

fn seeded() -> Store {
    let mut store = Store::new();
    store.apply("event/snapshot", &json!({"seq": 1, "state": example_status()}));
    store.apply("event/flush", &json!({"seq": 1}));
    store
}

#[test]
fn a_snapshot_is_not_on_screen_until_the_flush() {
    let mut store = Store::new();
    assert_eq!(
        store.apply("event/snapshot", &json!({"seq": 4, "state": example_status()})),
        Outcome::Staged
    );
    assert!(!store.view().ready, "the snapshot painted before its flush");
    assert!(store.view().status.sources.is_empty());
    assert_eq!(store.apply("event/flush", &json!({"seq": 4})), Outcome::Render);
    assert!(store.view().ready);
    assert_eq!(store.view().status.sources.len(), 2);
    assert_eq!(store.seq, 4);
}

#[test]
fn a_batch_of_deltas_lands_in_one_move() {
    let mut store = seeded();
    store.apply("event/program.took", &json!({"source": "cam1", "at_running_time_ms": 65000}));
    store.apply("event/source.state", &json!({"source": "clip1", "state": "stalled"}));
    store.apply("event/tally", &json!({"sources": {"cam1": "program", "clip1": "off"}}));
    // Half the batch is applied. None of it is on screen.
    let view = store.view();
    assert_eq!(view.status.program, None);
    assert_eq!(view.source("clip1").unwrap().state, SourceState::Live);
    assert!(view.tally.is_empty());

    assert_eq!(store.apply("event/flush", &json!({"seq": 9})), Outcome::Render);
    let view = store.view();
    assert_eq!(view.status.program.as_deref(), Some("cam1"));
    assert_eq!(view.status.running_time_ms, 65000);
    assert_eq!(view.source("clip1").unwrap().state, SourceState::Stalled);
    assert_eq!(view.tally_of("cam1"), "program");
}

#[test]
fn meters_and_positions_ride_the_same_flush() {
    let mut store = seeded();
    store.apply("event/meters", &json!({"program": [-12.5, -13.0], "sources": {"cam1": [-20.0]}}));
    store.apply(
        "event/source.position",
        &json!({"source": "clip1", "position_ms": 4000, "duration_ms": 30000}),
    );
    assert!(store.view().meters.program.is_empty());
    store.apply("event/flush", &json!({"seq": 12}));
    assert_eq!(store.view().meters.program, vec![-12.5, -13.0]);
    assert_eq!(store.view().source("clip1").unwrap().position_ms, Some(4000));
}

#[test]
fn the_alert_list_stops_at_fifty() {
    let mut store = seeded();
    for i in 0..60 {
        store.apply("event/alert", &json!({"severity": "warning", "message": format!("alert {i}")}));
    }
    store.apply("event/flush", &json!({"seq": 70}));
    let alerts = &store.view().alerts;
    assert_eq!(alerts.len(), godwinmix_tui::store::ALERT_LIMIT);
    assert_eq!(alerts.front().unwrap().message, "alert 59", "newest first");
}

#[test]
fn a_resync_asks_for_a_new_subscription() {
    let mut store = seeded();
    assert_eq!(
        store.apply("event/resync", &json!({"from_seq": 3, "dropped": 42})),
        Outcome::Resync
    );
}

#[test]
fn tally_falls_back_to_the_programme_until_the_first_one_arrives() {
    let mut store = seeded();
    store.apply("event/program.took", &json!({"source": "cam1"}));
    store.apply("event/flush", &json!({"seq": 5}));
    assert_eq!(store.view().tally_of("cam1"), "program");
    assert_eq!(store.view().tally_of("clip1"), "off");
}

#[test]
fn an_unknown_event_changes_nothing() {
    let mut store = seeded();
    assert_eq!(store.apply("event/scene.patch", &json!({"scene": "wide"})), Outcome::Ignored);
    assert_eq!(store.view().status.sources.len(), 2);
}
