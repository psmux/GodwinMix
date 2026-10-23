//! A page stands up on a headend that runs no mixer: the station answers
//! `core.info`, `core.subscribe` and the settings for the show that is not
//! running, says so with the feature `station`, and still refuses what
//! only a running show can do.

use super::support::*;
use serde_json::{json, Value};

const CONFIG: &str = "http 18999;
stream CH1 {
  input udp://127.0.0.1:9;
}
";

fn features(info: &Value) -> Vec<String> {
    info["features"].as_array().into_iter().flatten().filter_map(|f| f.as_str().map(String::from)).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_headend_answers_the_page_without_a_mixer() {
    let (dir, port) = folder("headend-page");
    let conf = dir.join("flussonic.conf");
    std::fs::write(&conf, CONFIG).unwrap();
    let facade = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port().to_string();
    let st = start(dir.clone(), port, &["headend", "--flussonic", conf.to_str().unwrap(), "--port", &facade]).await;

    let info = get(&st, "/api/v1/core/info").await;
    let f = features(&info);
    assert!(f.contains(&"station".into()) && f.contains(&"headend".into()), "{info}");
    assert_eq!(info["core"], "godwinmix", "{info}");

    let mut ws = rpc(&st, "?client_id=tab1").await;
    let info = call(&mut ws, 1, "core.info", json!({})).await;
    assert!(features(&info["result"]).contains(&"station".into()), "{info}");
    assert_eq!(info["result"]["client_id"], "open.tab1", "{info}");
    let sub = call(&mut ws, 2, "core.subscribe", json!({"events": ["show.*", "flush"]})).await;
    assert_eq!(sub["result"]["events"], json!(["show.*", "flush"]), "{sub}");

    let settings = call(&mut ws, 3, "config.get", json!({"keys": ["governor.nvenc_session_probe"]})).await;
    let path = settings["result"]["path"].as_str().unwrap_or_default().to_string();
    assert!(path.ends_with("godwinmix.toml"), "the station's own file: {settings}");
    assert_eq!(settings["result"]["keys"].as_array().map(Vec::len), Some(1), "{settings}");
    let schema = call(&mut ws, 4, "config.schema", json!({})).await;
    assert!(schema["result"].is_object(), "{schema}");
    let before = std::fs::read_to_string(dir.join("godwinmix.toml")).unwrap();
    let dry = call(&mut ws, 5, "config.set", json!({"values": {"governor.nvenc_session_probe": 64}, "dry_run": true})).await;
    assert_eq!(dry["result"]["dry_run"], true, "{dry}");
    assert_eq!(std::fs::read_to_string(dir.join("godwinmix.toml")).unwrap(), before, "a dry run writes nothing");
    let set = call(&mut ws, 6, "config.set", json!({"values": {"governor.nvenc_session_probe": 64}})).await;
    assert_eq!(set["result"]["needs_restart"], json!(["governor.nvenc_session_probe"]), "{set}");
    assert!(std::fs::read_to_string(dir.join("godwinmix.toml")).unwrap().contains("nvenc_session_probe = 64"));

    // The station's own methods answer as ever.
    let status = call(&mut ws, 7, "headend.status", json!({})).await;
    assert_eq!(status["result"]["following"], true, "{status}");
    // What only a mixer does is still refused, and says why.
    let scenes = call(&mut ws, 8, "scene.list", json!({})).await;
    assert!(scenes["error"]["message"].as_str().unwrap_or_default().contains("not running"), "{scenes}");
}
