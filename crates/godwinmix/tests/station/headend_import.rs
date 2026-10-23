//! `headend.import` through a real station: the report first with nothing
//! made, then each stream a show without compositing, a disabled one
//! stopped and a titled one named; a second import refused stream by
//! stream; and `headend.status` on a station that follows no file.

use super::support::*;
use serde_json::{json, Value};

const CONFIG: &str = "http 8090;
cluster_key abc;
stream CH117 {
  title News;
  input mpts-udp://eth1@239.10.0.1:2001?program=7;
  transcoder gop=50 hw=nvenc vb=1500k vcodec=h264 size=1280x720:fit:#000000 ab=128k;
}
stream RELAY1 {
  disabled true;
  input rtmp://127.0.0.1:1935/live/relay1;
}
";

fn ok(answer: &Value) -> &Value {
    assert!(answer.get("error").is_none(), "{answer}");
    &answer["result"]
}

fn show<'a>(list: &'a Value, id: &str) -> Option<&'a Value> {
    list["shows"].as_array()?.iter().find(|s| s["id"] == id)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_import_reports_first_then_makes_each_stream_a_show() {
    let (dir, port) = folder("headend-import");
    let st = start(dir.clone(), port, &[]).await;
    let mut ws = rpc(&st, "").await;

    let status = call(&mut ws, 1, "headend.status", json!({})).await;
    assert_eq!(ok(&status)["following"], false, "{status}");
    assert!(ok(&status)["note"].as_str().unwrap().contains("godwinmix headend"), "{status}");

    let dry = call(&mut ws, 2, "headend.import", json!({"text": CONFIG})).await;
    let r = ok(&dry);
    assert_eq!(r["dry_run"], true, "a dry run unless asked: {r}");
    assert_eq!((r["converted"].as_u64(), r["copied"].as_u64(), r["disabled"].as_u64()), (Some(1), Some(1), Some(1)), "{r}");
    assert_eq!(r["shows"]["added"], json!(["ch117", "relay1"]), "{r}");
    assert_eq!(r["streams"][0]["outputs"][0]["params"]["segments"], "both");
    assert!(r["notes"].as_array().unwrap().iter().any(|n| n["kind"] == "ignored" && n["what"].as_str().unwrap().contains("cluster_key")), "{r}");
    let list = call(&mut ws, 3, "show.list", json!({})).await;
    assert!(show(ok(&list), "ch117").is_none(), "a dry run makes nothing");

    let made = call(&mut ws, 4, "headend.import", json!({"text": CONFIG, "dry_run": false})).await;
    assert_eq!(ok(&made)["shows"]["added"].as_array().map(Vec::len), Some(2), "{made}");
    let list = call(&mut ws, 5, "show.list", json!({})).await;
    let news = show(ok(&list), "ch117").expect("CH117 is a show").clone();
    assert_eq!((news["name"].as_str(), news["compositing"].as_bool()), (Some("News"), Some(false)), "{news}");
    assert_eq!(news["input"]["program"], 7, "{news}");
    let out = &news["outputs"][0];
    assert_eq!(out["id"], "flussonic", "{news}");
    assert_eq!(out["rendition"]["video"]["keyframe_frames"], 50, "{news}");
    assert_eq!(out["rendition"]["audio"]["tracks"], "all", "{news}");
    let relay = show(ok(&list), "relay1").expect("RELAY1 is a show");
    assert_eq!(relay["state"], "stopped", "disabled true: made and stopped: {relay}");

    let again = call(&mut ws, 6, "headend.import", json!({"text": CONFIG})).await;
    assert_eq!(ok(&again)["shows"]["refused"].as_array().map(Vec::len), Some(2), "each stream is a show already: {again}");

    let broken = call(&mut ws, 7, "headend.import", json!({"text": "http 8090;\nstream A {\n"})).await;
    assert_eq!(broken["error"]["code"], -32602, "{broken}");
    let neither = call(&mut ws, 8, "headend.import", json!({})).await;
    assert!(neither["error"]["message"].as_str().unwrap().contains("text"), "{neither}");
    let missing = call(&mut ws, 9, "headend.import", json!({"path": dir.join("nope.conf")})).await;
    assert!(missing["error"]["message"].as_str().unwrap().contains("could not read"), "{missing}");
}
