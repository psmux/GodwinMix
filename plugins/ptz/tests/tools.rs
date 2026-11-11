//! The tools against a fake VISCA camera over TCP and a fake mixer, both on
//! loopback: the joystick that stops by itself, presets with pictures, and
//! the follow loop steering a camera after a person in a recorded scene.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use godwinmix_ptz::fake;
use gmx_ptz::core::Core;
use gmx_ptz::rig::Rig;
use std::sync::Arc;

use gmx_ptz::tools::{call, Shared};
use gmx_ptz::Settings;
use serde_json::{json, Value};

fn temp_dir(name: &str) -> String {
    let d = std::env::temp_dir().join(format!("gmx-ptz-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d.to_string_lossy().to_string()
}

/// A rig with one VISCA camera over TCP on a fake head, and a fake mixer.
fn rig(name: &str) -> (Shared, fake::Shared) {
    let head = fake::shared();
    let at = fake::visca::serve_tcp(head.clone());
    let settings = Settings::from_value(&json!({"data_dir": temp_dir(name)}));
    let rig: Shared = Arc::new(Rig::new(settings, Core::from_url(&common::serve(), None)).into());
    let added = call(&rig, "camera_add", &json!({"id": "cam-pulpit", "protocol": "visca-tcp", "address": at.to_string(), "source": "pulpit"}));
    assert!(added.is_ok(), "{added:?}");
    (rig, head)
}

fn facts(rig: &Shared, tool: &str, a: Value) -> Value {
    let r = call(rig, tool, &a).unwrap_or_else(|e| panic!("{tool}: {}", e.message));
    r.structured_content.unwrap()
}

#[test]
fn the_joystick_stops_by_itself_when_calls_stop_coming() {
    let (rig, head) = rig("joystick");
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(600) {
        facts(&rig, "move", json!({"pan": 1.0}));
        sleep(Duration::from_millis(100));
    }
    let moving = head.lock().unwrap().velocity;
    assert!(moving.pan > 0.9, "held by repeated calls: {moving:?}");
    sleep(Duration::from_millis(450));
    let after = head.lock().unwrap().velocity;
    assert_eq!(after.pan, 0.0, "stopped 300 ms after the last call");
    let status = rig.lock().unwrap().cameras["cam-pulpit"].handle.status.lock().unwrap().clone();
    assert_eq!(status.watchdog_stops, 1);
}

#[test]
fn a_preset_is_saved_with_a_picture_and_recalled() {
    let (rig, head) = rig("presets");
    head.lock().unwrap().jump(0.3, 0.1, 0.2);
    let saved = facts(&rig, "preset_save", json!({"camera": "cam-pulpit", "preset": 2, "name": "Pulpit"}));
    assert_eq!(saved["thumbnail_problem"], Value::Null, "{saved}");
    let jpg = std::fs::read(saved["thumbnail"].as_str().unwrap()).unwrap();
    assert_eq!(&jpg[..2], &[0xFF, 0xD8], "a JPEG");
    head.lock().unwrap().jump(-0.5, 0.0, 0.0);
    facts(&rig, "preset_recall", json!({"camera": "cam-pulpit", "preset": 2}));
    assert!((head.lock().unwrap().pan - 0.3).abs() < 0.01);
    let list = facts(&rig, "preset_list", json!({"camera": "cam-pulpit", "pictures": true}));
    assert_eq!(list["presets"][0]["name"], "Pulpit");
    assert!(list["presets"][0]["thumbnail_data"].as_str().unwrap().starts_with("data:image/jpeg;base64,"));
}

#[test]
fn the_follow_loop_pans_towards_the_person_and_says_who_it_follows() {
    let (rig, head) = rig("follow");
    let seen = facts(&rig, "track_observe", json!({"camera": "cam-pulpit", "wait_ms": 3000}));
    assert!(!seen["tracks"].as_array().unwrap().is_empty(), "somebody in view: {seen}");
    assert_eq!(seen["mode"], "observing");
    assert_eq!(head.lock().unwrap().moves, 0, "observing never moves the camera");

    facts(&rig, "track_follow", json!({"camera": "cam-pulpit", "framing": "full"}));
    let until = Instant::now() + Duration::from_secs(6);
    while Instant::now() < until && head.lock().unwrap().pan < 0.02 {
        sleep(Duration::from_millis(100));
    }
    let pan = head.lock().unwrap().pan;
    assert!(pan > 0.02, "the person is right of centre, so the camera pans right: {pan}");
    let state = facts(&rig, "track_state", json!({"camera": "cam-pulpit"}));
    assert_eq!(state["mode"], "following");
    assert_eq!(state["framing"], "full");
    assert!(state["target"].as_str().is_some_and(|t| t.starts_with("person-")), "{state}");

    facts(&rig, "track_stop", json!({"camera": "cam-pulpit"}));
    sleep(Duration::from_millis(300));
    assert_eq!(head.lock().unwrap().velocity.pan, 0.0, "stopping tracking stops the camera");
}

#[test]
fn a_camera_added_by_tool_comes_back_after_a_restart() {
    let dir = temp_dir("persist");
    let settings = Settings::from_value(&json!({"data_dir": dir}));
    let rig: Shared = Arc::new(Rig::new(settings.clone(), None).into());
    facts(&rig, "camera_add", json!({"id": "cam-demo", "protocol": "fake"}));
    drop(rig);
    let again: Shared = Arc::new(Rig::new(settings, None).into());
    let list = facts(&again, "camera_list", json!({}));
    assert_eq!(list["cameras"][0]["id"], "cam-demo");
    facts(&again, "camera_remove", json!({"camera": "cam-demo"}));
    assert!(Rig::new(Settings::from_value(&json!({"data_dir": dir})), None).cameras.is_empty());
}

#[test]
fn refusals_name_what_to_do() {
    let (rig, _) = rig("refusals");
    let e = call(&rig, "move", &json!({"camera": "cam-nave", "pan": 0.5})).unwrap_err();
    assert!(e.message.contains("cam-pulpit") && e.message.contains("camera_add"), "{}", e.message);
    let e = call(&rig, "move", &json!({"camera": "cam-pulpit", "pan": 2})).unwrap_err();
    assert!(e.message.contains("-1 to 1"), "{}", e.message);
    let e = call(&rig, "camera_add", &json!({"id": "Cam 2", "protocol": "visca-ip", "address": "192.0.2.9"})).unwrap_err();
    assert!(e.message.contains("slug"), "{}", e.message);
    let e = call(&rig, "track_follow", &json!({"camera": "cam-pulpit", "target": "bob"})).unwrap_err();
    assert!(e.message.contains("track_observe"), "{}", e.message);
    let e = call(&rig, "no_such", &json!({})).unwrap_err();
    assert!(e.message.contains("track_follow"), "{}", e.message);
}
