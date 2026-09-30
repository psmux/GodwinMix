//! `show.add {from: {project}}`: a project exported from one show opens as a
//! new one, through the public `project.import`.

use super::support::*;
use serde_json::{json, Value};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_project_from_one_show_opens_as_another() {
    let (dir, port) = folder("project");
    let st = start(dir, port, &[]).await;
    let mut ws = rpc(&st, "").await;
    call(&mut ws, 1, "core.subscribe", json!({"events": ["show.*"]})).await;
    let made = call(&mut ws, 2, "scene.add", json!({"name": "Carried over"})).await;
    assert!(made.get("result").is_some(), "{made}");
    let exported = call(&mut ws, 3, "project.export", json!({"name": "Sunday"})).await;
    let mut project = exported["result"].clone();
    assert!(project.is_object(), "{exported}");
    project.as_object_mut().unwrap().remove("trace_id");

    let added = call(&mut ws, 4, "show.add", json!({"name": "Opened", "from": {"project": project}})).await;
    assert_eq!(added["result"]["id"], "opened", "{added}");
    let running = |p: &Value| p["show"]["id"] == "opened" && p["show"]["state"] == "running";
    let _ = event(&mut ws, "show.changed", Duration::from_secs(5), running).await;
    let scenes = get(&st, "/api/v1/scenes?show=opened").await.to_string();
    assert!(scenes.contains("Carried over"), "the project's scene is in the new show: {scenes}");
}
