//! Every tool against a fake switcher on loopback: the answers the camera
//! window reads, and the bytes each control tool puts on the wire.

#[path = "../../../crates/godwinmix-atem/tests/fake/mod.rs"]
mod fake;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use gmx_atem::desk::Desk;
use gmx_atem::tools::{call, Shared};
use godwinmix_atem::{Command, RateStyle, Timing, TransitionStyle};
use serde_json::{json, Value};

fn desk() -> Shared {
    Arc::new(Mutex::new(Desk::new(tokio::runtime::Handle::current(), Timing::default().faster(10))))
}

fn ok(desk: &Shared, tool: &str, args: Value) -> Value {
    let r = call(desk, tool, &args).unwrap_or_else(|e| panic!("{tool} {args}: {}", e.message));
    r.structured_content.unwrap()
}

async fn up(desk: &Shared, id: &str) {
    for _ in 0..500 {
        if ok(desk, "state", json!({"switcher": id}))["connected"] == true {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("{id} did not come up");
}

#[tokio::test(flavor = "multi_thread")]
async fn connect_list_and_state_answer_what_the_window_reads() {
    let f = fake::Fake::start(0, 0).await;
    let d = desk();
    let map = json!({"cam-a": 1, "cam-b": 2});
    let c = ok(&d, "connect", json!({"id": "atem-main", "address": f.addr.to_string(), "name": "Main switcher", "map": map}));
    assert_eq!(c["id"], "atem-main");
    up(&d, "atem-main").await;
    let list = ok(&d, "list", json!({}));
    let row = &list["switchers"][0];
    assert_eq!((row["id"].as_str(), row["name"].as_str(), row["mode"].as_str()), (Some("atem-main"), Some("Main switcher"), Some("own")));
    assert_eq!((row["connected"].as_bool(), row["why"].is_null()), (Some(true), true));

    let s = ok(&d, "state", json!({}));
    assert_eq!(s["product"], "ATEM Mini Pro");
    assert_eq!((s["protocol"].as_str(), s["firmware"].as_str()), (Some("2.30"), Some("8.1.1")));
    assert_eq!(s["inputs"][1], json!({"id": 1, "long": "Camera 1", "short": "CAM1"}));
    let me = &s["mes"][0];
    assert_eq!((me["program"].as_u64(), me["preview"].as_u64()), (Some(1), Some(2)));
    assert_eq!(me["transition"]["style"], "mix");
    assert_eq!(me["transition"]["rate"], 25);
    assert_eq!(me["transition"]["position"], 0.0);
    assert_eq!(me["usk"], json!([{"keyer": 0, "on_air": false}]));
    assert_eq!(me["ftb"]["on"], false);
    assert_eq!((s["dsk"][0]["on_air"].as_bool(), s["dsk"][0]["tie"].as_bool()), (Some(false), Some(false)));
    assert_eq!(s["aux"], json!([{"aux": 0, "input": 10010}]));
    assert_eq!(s["macros"][0], json!({"index": 0, "name": "Opening", "description": "Wide, then the title"}));
    assert_eq!(s["tally"]["sources"], json!({"cam-a": "program", "cam-b": "preview"}));
    assert_eq!(s["map"], map);
    f.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn each_control_tool_puts_its_command_on_the_wire() {
    let f = fake::Fake::start(0, 0).await;
    let d = desk();
    ok(&d, "connect", json!({"id": "atem-main", "address": f.addr.to_string(), "map": {"cam-c": 3}}));
    up(&d, "atem-main").await;
    let calls: Vec<(&str, Value, Vec<Command>)> = vec![
        ("preview", json!({"input": "cam-c"}), vec![Command::Preview { me: 0, source: 3 }]),
        ("program", json!({"me": 0, "input": 4}), vec![Command::Program { me: 0, source: 4 }]),
        ("cut", json!({"switcher": "atem-main", "me": 0}), vec![Command::Cut { me: 0 }]),
        ("auto", json!({}), vec![Command::Auto { me: 0 }]),
        ("dsk", json!({"keyer": 0, "action": "auto"}), vec![Command::DskAuto { dsk: 0 }]),
        ("dsk", json!({"keyer": 0, "action": "on"}), vec![Command::DskOnAir { dsk: 0, on_air: true }]),
        ("dsk", json!({"keyer": 0, "action": "tie"}), vec![Command::DskTie { dsk: 0, tie: true }]),
        ("usk", json!({"me": 0, "keyer": 0, "on": true}), vec![Command::UskOnAir { me: 0, keyer: 0, on_air: true }]),
        ("ftb", json!({}), vec![Command::FadeToBlack { me: 0 }]),
        ("aux", json!({"aux": 0, "input": 2}), vec![Command::AuxSource { aux: 0, source: 2 }]),
        ("macro", json!({"index": 3, "action": "run"}), vec![Command::MacroRun { index: 3 }]),
        ("macro", json!({"action": "stop"}), vec![Command::MacroStop]),
        (
            "transition",
            json!({"me": 0, "style": "dip", "rate": 40, "position": 0.25}),
            vec![
                Command::TransitionStyle { me: 0, style: TransitionStyle::Dip },
                Command::TransitionRate { me: 0, style: RateStyle::Dip, frames: 40 },
                Command::TransitionPosition { me: 0, position: 2500 },
            ],
        ),
    ];
    let mut expected = Vec::new();
    for (tool, args, commands) in &calls {
        ok(&d, tool, args.clone());
        expected.extend(commands.iter().map(Command::encode));
        assert_eq!(f.wait_blocks(expected.len()).await, expected, "after {tool} {args}");
    }
    assert_eq!(ok(&d, "state", json!({}))["macro"]["last_run"], 3);
    f.stop().await;
}
