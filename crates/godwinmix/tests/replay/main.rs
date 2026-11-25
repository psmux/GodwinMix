//! Replay through the method table, as the desk, the CLI and an agent call it:
//! turning it on, a mark, a play on air and a change of speed, and a save that
//! the media library lists.

mod core;

use crate::core::Core;
use serde_json::json;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mark_play_and_save_through_the_methods() {
    let core = Core::start("methods").await;
    let off = core.refused("replay.play", json!({})).await;
    assert_eq!(off.data["next"], "replay.start", "replay off says how to turn it on: {off:?}");

    let on = core.ok("replay.start", json!({ "seconds": 30, "angles": ["programme", "cam1"] })).await;
    assert_eq!(on["on"], true);
    tokio::time::sleep(Duration::from_secs(5)).await;
    let got = core.ok("replay.get", json!({})).await;
    for angle in got["angles"].as_array().unwrap() {
        assert!(angle["held_secs"].as_f64().unwrap() > 3.0, "{angle}");
    }

    let event = core.ok("replay.mark", json!({ "seconds": 3, "name": "Goal" })).await;
    assert_eq!(event["id"], "replay-1");
    let renamed = core.ok("replay.event.rename", json!({ "id": "replay-1", "name": "Late goal" })).await;
    assert_eq!(renamed["name"], "Late goal");
    let wrong = core.refused("replay.play", json!({ "event": "replay-1", "angle": "cam9" })).await;
    assert_eq!(wrong.data["valid"], json!(["programme", "cam1"]), "{wrong:?}");
    let fast = core.refused("replay.play", json!({ "event": "replay-1", "speed": 2 })).await;
    assert_eq!(fast.data["field"], "speed");

    let playing = core.ok("replay.play", json!({ "event": "replay-1", "angle": "cam1", "take": true, "transition": "fade" })).await;
    assert_eq!((playing["source"].as_str(), playing["angle"].as_str()), (Some("replay"), Some("cam1")));
    assert!(playing["duration_ms"].as_u64().unwrap() >= 2900, "{playing}");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let program = core.ok("program.get", json!({})).await;
    assert!(program.to_string().contains("\"replay\""), "the replay is on air: {program}");
    let slow = core.ok("replay.set", json!({ "speed": 0.5 })).await;
    assert_eq!(slow["speed"], 0.5);
    assert!(slow["duration_ms"].as_u64().unwrap() >= 5800, "{slow}");

    let saved = core.ok("replay.save", json!({ "event": "replay-1", "name": "late goal" })).await;
    assert_eq!(saved["name"], "Replays/late goal.mp4");
    assert!(saved["duration_ms"].as_u64().unwrap() >= 2900, "{saved}");
    let buffer = core.ok("replay.save", json!({})).await;
    assert_eq!(buffer["angle"], "programme", "the whole buffer is the programme's by default");
    let media = core.ok("media.list", json!({})).await;
    let names: Vec<&str> = media["items"].as_array().unwrap().iter().filter_map(|i| i["name"].as_str()).collect();
    assert!(names.contains(&"Replays/late goal.mp4"), "the save is in the media library: {names:?}");

    let stopped = core.ok("replay.stop", json!({})).await;
    assert_eq!(stopped["on"], false);
    assert_eq!(core.ok("replay.event.list", json!({})).await["events"], json!([]));
}
