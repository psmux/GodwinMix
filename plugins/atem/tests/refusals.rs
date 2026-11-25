//! Two switchers at once, and the refusals that say what to do.

#[path = "../../../crates/godwinmix-atem/tests/fake/mod.rs"]
mod fake;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use gmx_atem::desk::Desk;
use gmx_atem::tools::{call, Shared};
use godwinmix_atem::Timing;
use serde_json::{json, Value};

fn desk() -> Shared {
    Arc::new(Mutex::new(Desk::new(tokio::runtime::Handle::current(), Timing::default().faster(10))))
}

fn ok(desk: &Shared, tool: &str, args: Value) -> Value {
    call(desk, tool, &args).unwrap_or_else(|e| panic!("{tool} {args}: {}", e.message)).structured_content.unwrap()
}

fn refused(desk: &Shared, tool: &str, args: Value) -> String {
    call(desk, tool, &args).err().unwrap_or_else(|| panic!("{tool} {args} was not refused")).message
}

async fn all_up(d: &Shared) {
    for _ in 0..500 {
        let l = ok(d, "list", json!({}));
        if l["switchers"].as_array().unwrap().iter().all(|s| s["connected"] == true) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the switchers did not come up");
}

#[tokio::test(flavor = "multi_thread")]
async fn two_switchers_each_under_its_slug() {
    let (a, b) = (fake::Fake::start(0, 0).await, fake::Fake::start(0, 0).await);
    let d = desk();
    assert!(refused(&d, "cut", json!({})).contains("no switcher is connected"));
    ok(&d, "connect", json!({"id": "atem-main", "address": a.addr.to_string()}));
    ok(&d, "connect", json!({"id": "atem-overflow", "address": b.addr.to_string(), "mode": "follow"}));
    all_up(&d).await;
    assert!(refused(&d, "cut", json!({})).contains("say which one"));
    ok(&d, "cut", json!({"switcher": "atem-overflow"}));
    assert_eq!(b.wait_blocks(1).await[0][4..8], *b"DCut");
    assert!(a.blocks().is_empty(), "the other switcher was not touched");
    assert!(refused(&d, "connect", json!({"id": "atem-main", "address": "192.0.2.10"})).contains("already connected"));
    assert!(refused(&d, "connect", json!({"id": "Atem Main", "address": "192.0.2.10"})).contains("not a switcher id"));
    assert!(refused(&d, "state", json!({"switcher": "atem-nope"})).contains("atem-main, atem-overflow"));
    ok(&d, "disconnect", json!({"switcher": "atem-overflow"}));
    assert_eq!(ok(&d, "list", json!({}))["switchers"].as_array().unwrap().len(), 1);
    a.stop().await;
    b.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_arguments_and_a_switcher_that_is_down() {
    let f = fake::Fake::start(0, 0).await;
    let d = desk();
    ok(&d, "connect", json!({"id": "atem-main", "address": f.addr.to_string()}));
    all_up(&d).await;
    assert!(refused(&d, "preview", json!({"input": 9})).contains("no input 9"));
    assert!(refused(&d, "preview", json!({"input": "cam-x"})).contains("not mapped"));
    assert!(refused(&d, "dsk", json!({"keyer": 3, "action": "on"})).contains("1 downstream keyers"));
    assert!(refused(&d, "dsk", json!({"keyer": 0, "action": "flash"})).contains("on, off, auto or tie"));
    assert!(refused(&d, "transition", json!({"style": "spin"})).contains("mix, dip, wipe, dve or sting"));
    assert!(refused(&d, "transition", json!({"position": 2})).contains("0 to 1"));
    assert!(refused(&d, "transition", json!({"style": "dve", "rate": 20})).contains("not supported yet"));
    assert!(refused(&d, "macro", json!({"index": 7})).contains("no macro in slot 7"));
    assert!(refused(&d, "mode", json!({"mode": "chase"})).contains("own, follow or lead"));
    assert_eq!(ok(&d, "map", json!({"source": "cam-a", "input": 1}))["map"], json!({"cam-a": 1}));
    assert_eq!(ok(&d, "mode", json!({"mode": "lead"}))["mode"], "lead");
    assert_eq!(ok(&d, "list", json!({}))["switchers"][0]["mode"], "lead");
    f.stop().await;
    let mut down = String::new();
    for _ in 0..300 {
        match call(&d, "cut", &json!({})) {
            Err(e) => {
                down = e.message;
                break;
            }
            Ok(_) => tokio::time::sleep(Duration::from_millis(10)).await,
        }
    }
    assert!(down.contains("was not sent"), "{down}");
    let why = ok(&d, "list", json!({}))["switchers"][0]["why"].as_str().unwrap_or_default().to_string();
    assert!(!why.is_empty(), "list says why it is down");
}
