//! The teleprompter against a real mixer: scripts and their refusals, the
//! moves every screen follows, the place kept across a restart, and one
//! change reaching every screen that asked for it and no other client.

mod core;
mod fanout;
mod screen;

use crate::core::Core;
use godwinmix_protocol::prompter::PrompterState;
use serde_json::{json, Value};
use std::time::Duration;

pub const SCRIPT: &str = "# Opening\nGood evening and welcome to the news at six.\n\n# Weather\nRain in the west by morning.\n# Sport\nA late goal at Elland Road.";

pub fn state(v: Value) -> PrompterState {
    serde_json::from_value(v).expect("a prompter state")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scripts_are_kept_by_slug_and_bad_ones_say_what_to_send() {
    let core = Core::start("scripts").await;
    let saved = core.ok("prompter.script.save", json!({ "title": "Evening News", "text": SCRIPT })).await;
    assert_eq!(saved["id"], "evening-news", "the id is made from the title: {saved}");
    let list = core.ok("prompter.script.list", json!({})).await;
    assert_eq!(list["scripts"][0]["markers"], json!(["Opening", "Weather", "Sport"]));
    assert_eq!(list["scripts"][0]["lines"], 7);
    assert!(list["on"].is_null());
    assert_eq!(core.ok("prompter.script.get", json!({ "id": "evening-news" })).await["text"], SCRIPT);

    let bad = core.refused("prompter.script.save", json!({ "id": "Evening News", "title": "x", "text": "" })).await;
    assert_eq!(bad.code, -32602);
    assert_eq!(bad.data["suggest"], "evening-news", "{bad:?}");
    let untitled = core.refused("prompter.script.save", json!({ "id": "a", "title": " ", "text": "" })).await;
    assert_eq!(untitled.data["field"], "title");
    let big = "word ".repeat(120_000);
    let huge = core.refused("prompter.script.save", json!({ "id": "big", "title": "Big", "text": big })).await;
    assert_eq!(huge.data["max_bytes"], 512 * 1024);
    let missing = core.refused("prompter.script.get", json!({ "id": "late-news" })).await;
    assert_eq!(missing.code, -32004);
    assert_eq!(missing.data["scripts"], json!(["evening-news"]), "{missing:?}");
    let typo = core.refused("prompter.script.save", json!({ "title": "x", "text": "", "body": "" })).await;
    assert_eq!(typo.code, -32602, "an unknown field is refused");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn play_pause_speed_and_seek_move_one_shared_position() {
    let core = Core::start("moves").await;
    let empty = core.refused("prompter.play", json!({})).await;
    assert_eq!(empty.code, -32001, "nothing is loaded: {empty:?}");
    assert_eq!(empty.data["state"], "empty");

    core.ok("prompter.script.save", json!({ "id": "news", "title": "News", "text": SCRIPT })).await;
    let loaded = state(core.ok("prompter.load", json!({ "id": "news" })).await);
    assert!(!loaded.playing && loaded.position == 0.0);
    let names: Vec<&str> = loaded.markers.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["Opening", "Weather", "Sport"]);

    let playing = state(core.ok("prompter.play", json!({ "speed": 4.0 })).await);
    assert!(playing.playing && playing.speed == 4.0);
    tokio::time::sleep(Duration::from_millis(400)).await;
    let paused = state(core.ok("prompter.pause", json!({})).await);
    assert!(!paused.playing);
    assert!(paused.position > 1.0 && paused.position < 3.5, "four rows a second for 0.4 s: {}", paused.position);
    assert!(paused.revision > playing.revision);

    let nudged = state(core.ok("prompter.speed", json!({ "by": -0.5 })).await);
    assert_eq!(nudged.speed, 3.5);
    let fast = core.refused("prompter.speed", json!({ "speed": 50 })).await;
    assert_eq!(fast.data["max"], 20.0);
    let both = core.refused("prompter.speed", json!({ "speed": 1, "by": 1 })).await;
    assert_eq!(both.code, -32602);

    let weather = loaded.markers[1].row as f64;
    assert_eq!(state(core.ok("prompter.seek", json!({ "marker": "weather" })).await).position, weather);
    assert_eq!(state(core.ok("prompter.seek", json!({ "marker": "3" })).await).position, loaded.markers[2].row as f64);
    assert_eq!(state(core.ok("prompter.seek", json!({ "step": -1 })).await).position, weather);
    assert_eq!(state(core.ok("prompter.seek", json!({ "step": 1 })).await).position, loaded.markers[2].row as f64);
    assert_eq!(state(core.ok("prompter.seek", json!({ "fraction": 1.0 })).await).position, loaded.rows as f64);
    let nowhere = core.refused("prompter.seek", json!({ "marker": "Traffic" })).await;
    assert_eq!(nowhere.data["markers"], json!(["Opening", "Weather", "Sport"]));
    let two = core.refused("prompter.seek", json!({ "row": 1, "step": 1 })).await;
    assert_eq!(two.code, -32602);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_text_size_keeps_the_place_and_bad_looks_are_refused() {
    let core = Core::start("look").await;
    core.ok("prompter.script.save", json!({ "id": "news", "title": "News", "text": SCRIPT })).await;
    core.ok("prompter.load", json!({ "id": "news" })).await;
    let at = state(core.ok("prompter.seek", json!({ "marker": "Sport" })).await);
    let bigger = state(core.ok("prompter.set", json!({ "line_chars": 14, "mirror": true })).await);
    assert!(bigger.rows > at.rows, "fewer characters a row makes more rows");
    assert_eq!(bigger.position, bigger.markers[2].row as f64, "still at the start of Sport");
    assert!(bigger.look.mirror);
    let rows = core.ok("prompter.rows", json!({})).await;
    assert_eq!(rows["line_chars"], 14);
    assert_eq!(rows["rows"].as_array().unwrap().len() as u32, bigger.rows);
    for (field, value) in [("line_chars", json!(4)), ("margin", json!(45)), ("cue", json!(0.95))] {
        let refused = core.refused("prompter.set", json!({ field: value })).await;
        assert_eq!(refused.data["field"], field, "{refused:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_restart_finds_the_scripts_and_the_place_paused() {
    let core = Core::start("restart").await;
    core.ok("prompter.script.save", json!({ "id": "news", "title": "News", "text": SCRIPT })).await;
    core.ok("prompter.script.save", json!({ "id": "late", "title": "Late", "text": "Goodnight." })).await;
    core.ok("prompter.load", json!({ "id": "news" })).await;
    core.ok("prompter.set", json!({ "line_chars": 20, "margin": 12 })).await;
    let before = state(core.ok("prompter.seek", json!({ "marker": "Weather" })).await);
    core.ok("prompter.play", json!({ "speed": 0.8 })).await;

    let file = core.path.with_file_name("godwinmix.prompter.json");
    assert!(file.is_file(), "the scripts are kept beside the config");
    let reopened = godwinmix::prompter::Prompter::open(&core.path, core.app.mixer.clone());
    let after = reopened.state();
    assert_eq!(after.script.as_deref(), Some("news"));
    assert!(!after.playing, "a restart comes back paused");
    assert_eq!(after.position, before.position, "at the last place it was sent to");
    assert_eq!((after.look.line_chars, after.look.margin), (20, 12.0));
    assert_eq!(reopened.list().scripts.len(), 2);

    core.ok("prompter.script.remove", json!({ "id": "news" })).await;
    let emptied = godwinmix::prompter::Prompter::open(&core.path, core.app.mixer.clone()).state();
    assert!(emptied.script.is_none() && emptied.rows == 0, "removing the loaded script empties the prompter");
}
