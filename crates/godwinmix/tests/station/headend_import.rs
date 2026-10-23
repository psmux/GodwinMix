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

/// True when `id` was added, false when the governor refused it for room.
/// Any other refusal fails the test.
fn fits(import: &Value, id: &str) -> bool {
    let added = import["shows"]["added"].as_array().is_some_and(|a| a.iter().any(|s| s == id));
    if !added {
        let refused = import["shows"]["refused"].as_array().into_iter().flatten().find(|x| x["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(id)));
        assert_eq!(refused.map(|x| &x["data"]["alarm"]), Some(&json!("governor-refused")), "{id} was neither added nor refused for room: {import}");
    }
    added
}

fn show<'a>(list: &'a Value, id: &str) -> Option<&'a Value> {
    list["shows"].as_array()?.iter().find(|s| s["id"] == id)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_import_reports_first_then_makes_each_stream_a_show() {
    let (dir, port) = folder("headend-import");
    // CH117 is converted, about a core of x264 on a runner with no GPU, and
    // the import refuses a stream the machine has no room for. Alone and
    // with no reserve, as hls_direct.rs and isolation.rs do, so it fits
    // wherever it can; a four core runner still had only 741 millicores
    // free with the station to itself. What is checked here is the import,
    // not the governor, so a refusal for room is taken as the right answer
    // and CH117's conversion is then checked in the report alone.
    let config = dir.join("godwinmix.toml");
    let text = std::fs::read_to_string(&config).unwrap();
    std::fs::write(&config, format!("{text}\n[governor]\nreserve_cores = 0\n")).unwrap();
    let st = start_alone(dir.clone(), port, &[]).await;
    let mut ws = rpc(&st, "").await;

    let status = call(&mut ws, 1, "headend.status", json!({})).await;
    assert_eq!(ok(&status)["following"], false, "{status}");
    assert!(ok(&status)["note"].as_str().unwrap().contains("godwinmix headend"), "{status}");

    let dry = call(&mut ws, 2, "headend.import", json!({"text": CONFIG})).await;
    let r = ok(&dry);
    assert_eq!(r["dry_run"], true, "a dry run unless asked: {r}");
    assert_eq!((r["converted"].as_u64(), r["copied"].as_u64(), r["disabled"].as_u64()), (Some(1), Some(1), Some(1)), "{r}");
    let room = fits(r, "ch117");
    let both: Vec<&str> = if room { vec!["ch117", "relay1"] } else { vec!["relay1"] };
    assert_eq!(r["shows"]["added"], json!(both), "{r}");
    assert_eq!(r["streams"][0]["outputs"][0]["params"]["segments"], "both");
    let rendition = &r["streams"][0]["outputs"][0]["rendition"];
    assert_eq!((rendition["video"]["keyframe_frames"].as_u64(), rendition["video"]["width"].as_u64()), (Some(50), Some(1280)), "{r}");
    assert_eq!(rendition["audio"]["tracks"], "all", "{r}");
    assert!(r["notes"].as_array().unwrap().iter().any(|n| n["kind"] == "ignored" && n["what"].as_str().unwrap().contains("cluster_key")), "{r}");
    let list = call(&mut ws, 3, "show.list", json!({})).await;
    assert!(show(ok(&list), "ch117").is_none(), "a dry run makes nothing");

    let made = call(&mut ws, 4, "headend.import", json!({"text": CONFIG, "dry_run": false})).await;
    let room = fits(ok(&made), "ch117");
    assert_eq!(ok(&made)["shows"]["added"].as_array().map(Vec::len), Some(if room { 2 } else { 1 }), "{made}");
    let list = call(&mut ws, 5, "show.list", json!({})).await;
    if room {
        let news = show(ok(&list), "ch117").expect("CH117 is a show").clone();
        assert_eq!((news["name"].as_str(), news["compositing"].as_bool()), (Some("News"), Some(false)), "{news}");
        assert_eq!(news["input"]["program"], 7, "{news}");
        let out = &news["outputs"][0];
        assert_eq!(out["id"], "flussonic", "{news}");
        assert_eq!(out["rendition"]["video"]["keyframe_frames"], 50, "{news}");
        assert_eq!(out["rendition"]["audio"]["tracks"], "all", "{news}");
    } else {
        eprintln!("CH117 was refused for room on this machine, so only the report's rendition was checked: {made}");
        assert!(show(ok(&list), "ch117").is_none(), "a refused stream makes no show");
    }
    let relay = show(ok(&list), "relay1").expect("RELAY1 is a show");
    assert_eq!(relay["state"], "stopped", "disabled true: made and stopped: {relay}");

    let again = call(&mut ws, 6, "headend.import", json!({"text": CONFIG})).await;
    if room {
        assert_eq!(ok(&again)["shows"]["refused"].as_array().map(Vec::len), Some(2), "each stream is a show already: {again}");
    } else {
        assert!(!ok(&again)["shows"]["added"].as_array().unwrap().iter().any(|s| s == "relay1"), "RELAY1 is a show already: {again}");
    }

    let broken = call(&mut ws, 7, "headend.import", json!({"text": "http 8090;\nstream A {\n"})).await;
    assert_eq!(broken["error"]["code"], -32602, "{broken}");
    let neither = call(&mut ws, 8, "headend.import", json!({})).await;
    assert!(neither["error"]["message"].as_str().unwrap().contains("text"), "{neither}");
    let missing = call(&mut ws, 9, "headend.import", json!({"path": dir.join("nope.conf")})).await;
    assert!(missing["error"]["message"].as_str().unwrap().contains("could not read"), "{missing}");
}
