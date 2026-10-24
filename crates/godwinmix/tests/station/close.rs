//! Closing a show from its tab, as quitting closes the app: nothing of it is
//! deleted, it stays closed across a restart of the station, and only one
//! kept running comes back running. Shows without compositing, so no show
//! process has to start for the states to be read.

use super::support::*;
use serde_json::{json, Value};

fn ok(answer: &Value) -> &Value {
    assert!(answer.get("error").is_none(), "{answer}");
    &answer["result"]
}

fn direct(name: &str, port: u16) -> Value {
    json!({
        "name": name, "compositing": false,
        "input": {"uri": format!("udp://@239.1.1.1:{port}")},
        "outputs": [{"id": "out", "uri": "srt://10.0.0.9:9000"}]
    })
}

fn row(list: &Value, id: &str) -> Value {
    list["shows"].as_array().unwrap().iter().find(|s| s["id"] == id).cloned().unwrap_or_else(|| panic!("no {id} in {list}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_closed_show_keeps_everything_and_only_one_kept_running_comes_back_running() {
    let (dir, port) = folder("close-show");
    let st = start(dir.clone(), port, &[]).await;
    let mut ws = rpc(&st, "").await;
    ok(&call(&mut ws, 1, "show.add", direct("Kids room", 5001)).await);
    ok(&call(&mut ws, 2, "show.add", direct("Overflow", 5002)).await);

    let closed = call(&mut ws, 3, "show.close", json!({"id": "kids-room"})).await;
    assert_eq!(ok(&closed)["closed"], true);
    assert_eq!(ok(&closed)["state"], "stopped", "closing without keep_running stops it: {closed}");
    let kept = call(&mut ws, 4, "show.close", json!({"id": "overflow", "keep_running": true})).await;
    assert_eq!(ok(&kept)["closed"], true);
    assert_eq!(ok(&kept)["state"], "running", "kept running carries on: {kept}");
    assert_eq!(ok(&kept)["outputs"].as_array().map(Vec::len), Some(1), "nothing of it went");

    let last = call(&mut ws, 5, "show.close", json!({"id": "main"})).await;
    assert_eq!(last["error"]["data"]["show"], "main", "the last open show stays open: {last}");
    let list = get(&st, "/api/v1/shows").await;
    assert_eq!(row(&list, "main")["closed"], false);
    assert_eq!(list["shows"].as_array().map(Vec::len), Some(3), "closing deletes no show: {list}");

    drop(ws);
    drop(st);
    let st = start(dir.clone(), port, &[]).await;
    let list = get(&st, "/api/v1/shows").await;
    let kids = row(&list, "kids-room");
    assert_eq!((kids["closed"].clone(), kids["state"].clone()), (json!(true), json!("stopped")), "not started with the station: {kids}");
    let over = row(&list, "overflow");
    assert_eq!((over["closed"].clone(), over["state"].clone()), (json!(true), json!("running")), "running, and still closed: {over}");
    assert_eq!(over["outputs"][0]["id"], "out");

    let mut ws = rpc(&st, "").await;
    let opened = call(&mut ws, 6, "show.open", json!({"id": "kids-room"})).await;
    assert_eq!((ok(&opened)["closed"].clone(), ok(&opened)["state"].clone()), (json!(false), json!("running")), "opening starts what closing stopped: {opened}");
    let main = call(&mut ws, 7, "show.close", json!({"id": "main"})).await;
    assert_eq!(ok(&main)["closed"], true, "main closes like any other while another show is open: {main}");
    assert_eq!(ok(&main)["state"], "stopped");
    let back = call(&mut ws, 8, "show.open", json!({"id": "overflow"})).await;
    assert_eq!((ok(&back)["closed"].clone(), ok(&back)["state"].clone()), (json!(false), json!("running")));
    let file = std::fs::read_to_string(dir.join("shows.json")).unwrap();
    assert!(file.contains("\"closed\": true"), "kept in the station's list: {file}");
}
