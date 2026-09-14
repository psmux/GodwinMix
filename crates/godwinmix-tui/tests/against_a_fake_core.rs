//! The whole surface against a fake core: what it subscribes to, what a key
//! sends, what a refusal looks like, and what happens when the link drops.

mod support;

use godwinmix_tui::app::Link;
use godwinmix_tui::client::MultiviewWant;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use support::{Fake, Harness, Push};

#[tokio::test]
async fn it_subscribes_for_meters_tally_and_positions_and_no_picture() {
    let fake = Fake::start().await;
    let mut ui = Harness::start(fake.clone(), None).await;
    assert!(ui.pump_until(|app| app.view().ready).await, "no snapshot arrived");

    let subscribe = ui.wait_for_call("core.subscribe").await;
    let params = &subscribe["params"];
    assert_eq!(params["events"], json!(["*"]));
    let ext = params["ext"].as_object().expect("an ext table");
    assert_eq!(ext.get("meters"), Some(&json!(true)));
    assert_eq!(ext.get("tally"), Some(&json!(true)));
    assert_eq!(ext.get("positions"), Some(&json!(true)));
    assert!(
        !ext.contains_key("multiview"),
        "the TUI asked for a mosaic nobody wanted: {ext:?}"
    );
    // Nothing anywhere in the request mentions the mosaic, so the core builds
    // none and gmx_multiview_subscribers stays at zero.
    assert!(!subscribe.to_string().contains("multiview"), "{subscribe}");

    ui.pump_ms(200).await;
    assert_eq!(ui.app.frames_seen, 0, "binary frames arrived without being asked for");
}

#[tokio::test]
async fn multiview_is_asked_for_only_when_the_flag_is_given() {
    let fake = Fake::start().await;
    let mut ui = Harness::start(fake.clone(), Some(MultiviewWant { fps: 4, width: 320 })).await;
    assert!(ui.pump_until(|app| app.view().ready).await);
    let subscribe = ui.wait_for_call("core.subscribe").await;
    assert_eq!(subscribe["params"]["ext"]["multiview"], json!({"fps": 4, "width": 320}));

    // And a binary frame is read as a frame, header and all.
    let mut frame = Vec::new();
    frame.extend_from_slice(&1u32.to_le_bytes());
    frame.extend_from_slice(&2u32.to_le_bytes());
    frame.extend_from_slice(&3000u64.to_le_bytes());
    frame.extend_from_slice(b"not-really-a-jpeg");
    fake.send(Push::Binary(frame));
    assert!(ui.pump_until(|app| app.frames_seen == 1).await, "the mosaic frame never arrived");
    let held = ui.app.frame.as_ref().unwrap();
    assert_eq!((held.seq, held.layout, held.running_time_ms), (1, 2, 3000));
}

#[tokio::test]
async fn a_number_key_takes_that_source() {
    let fake = Fake::start().await;
    fake.answer("program.take", json!({"program": "cam1", "running_time_ms": 61000}));
    let mut ui = Harness::start(fake.clone(), None).await;
    assert!(ui.pump_until(|app| app.view().ready).await);

    ui.press('1').await;
    let call = ui.wait_for_call("program.take").await;
    assert_eq!(call["params"], json!({"source": "cam1"}));
    assert!(ui.pump_until(|app| app.footer.text.contains("on air: cam1")).await);

    // The second source is the second row, and 0 is the slate.
    ui.press('2').await;
    ui.pump_ms(100).await;
    let takes = fake.calls("program.take");
    assert_eq!(takes[1]["params"], json!({"source": "clip1"}));
    ui.press('0').await;
    ui.pump_ms(100).await;
    let takes = fake.calls("program.take");
    assert_eq!(takes[2]["params"], json!({"source": Value::Null}));
}

#[tokio::test]
async fn the_filter_decides_what_the_number_keys_count() {
    let fake = Fake::start().await;
    let mut ui = Harness::start(fake.clone(), None).await;
    assert!(ui.pump_until(|app| app.view().ready).await);

    ui.press('/').await;
    for c in "clip".chars() {
        ui.press(c).await;
    }
    ui.press_enter().await;
    assert_eq!(ui.app.visible_sources().len(), 1);
    ui.press('1').await;
    let call = ui.wait_for_call("program.take").await;
    assert_eq!(call["params"], json!({"source": "clip1"}));
}

#[tokio::test]
async fn a_refusal_lands_on_the_footer_with_its_next_step() {
    let fake = Fake::start().await;
    let message = "source 'cam1' is not live (state: connecting). Live sources: clip1. \
                   Wait for event/source.state or take one of those.";
    fake.refuse("program.take", -32001, message);
    let mut ui = Harness::start(fake.clone(), None).await;
    assert!(ui.pump_until(|app| app.view().ready).await);

    ui.press('1').await;
    assert!(ui.pump_until(|app| app.footer.bad).await, "the refusal never reached the footer");
    assert!(ui.app.footer.text.contains("is not live"), "{}", ui.app.footer.text);
    assert!(ui.app.footer.text.contains("-32001"), "{}", ui.app.footer.text);
    // And it is kept in the alert list, because a refusal during a show is
    // worth being able to look back at.
    assert!(ui.app.view().alerts.iter().any(|a| a.message.contains("is not live")));
}

#[tokio::test]
async fn a_core_without_revert_says_so_once() {
    let fake = Fake::start().await;
    fake.refuse("program.revert", -32601, "no such method 'program.revert'");
    let mut ui = Harness::start(fake.clone(), None).await;
    assert!(ui.pump_until(|app| app.view().ready).await);

    ui.press('r').await;
    assert!(ui.pump_until(|app| app.revert_missing).await);
    // The second press does not go near the wire.
    ui.press('r').await;
    ui.pump_ms(100).await;
    assert_eq!(fake.calls("program.revert").len(), 1);
    assert!(ui.app.footer.text.contains("no program.revert"), "{}", ui.app.footer.text);
}

#[tokio::test]
async fn mute_and_the_fader_go_through_source_audio_set() {
    let fake = Fake::start().await;
    let mut ui = Harness::start(fake.clone(), None).await;
    assert!(ui.pump_until(|app| app.view().ready).await);

    ui.press('m').await;
    let call = ui.wait_for_call("source.audio.set").await;
    assert_eq!(call["params"], json!({"id": "cam1", "muted": true}));

    ui.press('-').await;
    ui.pump_ms(100).await;
    let calls = fake.calls("source.audio.set");
    let gain = calls[1]["params"]["gain"].as_f64().unwrap();
    // One decibel down from unity.
    assert!((gain - 0.891).abs() < 0.002, "{gain}");
}

#[tokio::test]
async fn an_ad_break_is_typed_in_and_then_ended() {
    let fake = Fake::start().await;
    let mut ui = Harness::start(fake.clone(), None).await;
    assert!(ui.pump_until(|app| app.view().ready).await);

    ui.press('a').await;
    for c in "/srv/ads/spot.mp4".chars() {
        ui.press(c).await;
    }
    ui.press_enter().await;
    let call = ui.wait_for_call("adbreak.start").await;
    assert_eq!(call["params"], json!({"uri": "/srv/ads/spot.mp4"}));

    // With one on air, the same key ends it.
    fake.event("event/adbreak.changed", json!({"ad": {"uri": "/srv/ads/spot.mp4", "on_air": true}}));
    fake.flush(20);
    assert!(ui.pump_until(|app| app.view().status.ad.is_some()).await);
    ui.press('a').await;
    ui.wait_for_call("adbreak.end").await;
}

#[tokio::test]
async fn the_outputs_pane_reconnects_a_destination() {
    let fake = Fake::start().await;
    let mut ui = Harness::start(fake.clone(), None).await;
    assert!(ui.pump_until(|app| app.view().ready).await);

    ui.press('o').await;
    ui.press('s').await;
    let call = ui.wait_for_call("output.reconnect").await;
    assert_eq!(call["params"], json!({"id": "youtube"}));
}

#[tokio::test]
async fn a_dropped_link_comes_back_and_subscribes_again() {
    let fake = Fake::start().await;
    let mut ui = Harness::start(fake.clone(), None).await;
    assert!(ui.pump_until(|app| app.view().ready).await);
    assert_eq!(fake.connections.load(Ordering::SeqCst), 1);

    fake.send(Push::Drop);
    assert!(
        ui.pump_until(|app| matches!(app.link, Link::Retrying { .. })).await,
        "the screen never said the link had gone"
    );
    assert!(!ui.app.view().ready, "the screen kept painting a mixer it could not see");

    assert!(
        ui.pump_until(|app| app.view().ready && app.link == Link::Live).await,
        "the link never came back"
    );
    assert_eq!(fake.connections.load(Ordering::SeqCst), 2);
    assert_eq!(fake.calls("core.subscribe").len(), 2, "it reconnected without subscribing again");
}

#[tokio::test]
async fn a_resync_re_subscribes_without_reconnecting() {
    let fake = Fake::start().await;
    let mut ui = Harness::start(fake.clone(), None).await;
    assert!(ui.pump_until(|app| app.view().ready).await);

    fake.event("event/resync", json!({"from_seq": 3, "dropped": 17}));
    assert!(ui.pump_until(|app| app.view().ready && app.store.seq >= 1).await);
    for _ in 0..100 {
        if fake.calls("core.subscribe").len() == 2 {
            break;
        }
        ui.pump_ms(20).await;
    }
    assert_eq!(fake.calls("core.subscribe").len(), 2, "a resync did not re-subscribe");
    assert_eq!(fake.connections.load(Ordering::SeqCst), 1, "it reconnected when it need not have");
    assert!(
        ui.app
            .view()
            .alerts
            .iter()
            .any(|a| a.message.contains("17 events were dropped")),
        "the resync was not written down anywhere the operator can see it"
    );
}

#[tokio::test]
async fn the_screen_follows_the_mixer_through_a_take() {
    let fake = Fake::start().await;
    let mut ui = Harness::start(fake.clone(), None).await;
    assert!(ui.pump_until(|app| app.view().ready).await);

    fake.event("event/program.took", json!({"source": "cam1", "at_running_time_ms": 70000}));
    fake.event("event/tally", json!({"sources": {"cam1": "program", "clip1": "off"}}));
    fake.event("event/meters", json!({"program": [-9.0], "sources": {"cam1": [-11.0]}}));
    fake.flush(31);
    assert!(ui.pump_until(|app| app.view().status.program.as_deref() == Some("cam1")).await);
    let view = ui.app.view();
    assert_eq!(view.tally_of("cam1"), "program");
    assert_eq!(view.meters.program, vec![-9.0]);
    assert_eq!(view.status.running_time_ms, 70000);
}
