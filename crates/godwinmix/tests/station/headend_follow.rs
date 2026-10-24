//! A followed Flussonic file and the headend screen side by side: a stream
//! edited on the screen is kept by the station and the file's later edits
//! to it are not applied until it is given back; a stream removed on the
//! screen is hidden from the file. And a headend with no file at all serves
//! the channels set up on it on Flussonic's URLs.

use super::support::*;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const CONFIG: &str = "stream CH1 {\n  input udp://127.0.0.1:9;\n}\nstream CH2 {\n  input udp://127.0.0.1:11;\n}\n";

fn ok(answer: &Value) -> &Value {
    assert!(answer.get("error").is_none(), "{answer}");
    &answer["result"]
}

fn entry<'a>(setup: &'a Value, name: &str) -> Option<&'a Value> {
    setup["channels"].as_array()?.iter().find(|c| c["channel"]["name"] == name)
}

async fn shows(ws: &mut Ws, id: u64) -> Value {
    ok(&call(ws, id, "show.list", json!({})).await).clone()
}

fn input_of(list: &Value, id: &str) -> Value {
    list["shows"].as_array().unwrap().iter().find(|s| s["id"] == id).map(|s| s["input"]["uri"].clone()).unwrap_or(Value::Null)
}

/// Ask until `test` holds, for at most 20 s.
async fn until(ws: &mut Ws, mut id: u64, method: &str, test: impl Fn(&Value) -> bool) -> Value {
    let started = Instant::now();
    loop {
        id += 1;
        let got = ok(&call(ws, id, method, json!({})).await).clone();
        if test(&got) || started.elapsed() > Duration::from_secs(20) {
            return got;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

async fn facade(port: &str, path: &str) -> (u16, String) {
    let r = reqwest::get(format!("http://127.0.0.1:{port}{path}")).await.unwrap();
    (r.status().as_u16(), r.text().await.unwrap_or_default())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_edit_on_the_screen_takes_a_stream_out_of_the_files_hands() {
    let (dir, port) = folder("headend-follow");
    let conf = dir.join("flussonic.conf");
    std::fs::write(&conf, CONFIG).unwrap();
    let fport = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port().to_string();
    let st = start(dir.clone(), port, &["headend", "--flussonic", conf.to_str().unwrap(), "--port", &fport]).await;
    let mut ws = rpc(&st, "").await;

    let setup = until(&mut ws, 100, "headend.channel.list", |s| entry(s, "CH1").is_some_and(|e| e["show"].is_string())).await;
    let ch1 = entry(&setup, "CH1").unwrap().clone();
    assert_eq!(ch1["follows"].as_str(), Some(conf.to_str().unwrap()), "{setup}");
    let mut c = ch1["channel"].clone();
    c["title"] = json!("First");
    let up = call(&mut ws, 1, "headend.channel.update", json!({"name": "CH1", "channel": c})).await;
    assert_eq!(ok(&up)["done"], json!(["CH1"]), "{up}");
    let setup = ok(&call(&mut ws, 2, "headend.channel.list", json!({})).await).clone();
    let ch1 = entry(&setup, "CH1").unwrap();
    assert!(ch1["follows"].is_null() && ch1["detached_from"].is_string(), "{setup}");
    assert_eq!(ch1["show"], "ch1", "handed over, not made again: {setup}");

    // The file moves CH1's input and names CH2: only CH2 follows.
    std::fs::write(&conf, CONFIG.replace("127.0.0.1:9", "127.0.0.1:10").replace("stream CH2 {\n", "stream CH2 {\n  title Second;\n")).unwrap();
    let list = until(&mut ws, 200, "show.list", |l| l["shows"].as_array().unwrap().iter().any(|s| s["name"] == "Second")).await;
    assert_eq!(input_of(&list, "ch1"), "udp://127.0.0.1:9", "the file's edit to a taken stream is not applied: {list}");

    // Both are on Flussonic's API, the taken one in the station's words.
    let (code, body) = facade(&fport, "/streamer/api/v3/streams").await;
    assert_eq!(code, 200, "{body}");
    assert!(body.contains("\"CH1\"") && body.contains("\"CH2\""), "{body}");

    ok(&call(&mut ws, 3, "headend.channel.follow", json!({"names": ["CH1"]})).await);
    let list = until(&mut ws, 300, "show.list", |l| input_of(l, "ch1") == "udp://127.0.0.1:10").await;
    assert_eq!(input_of(&list, "ch1"), "udp://127.0.0.1:10", "given back, it runs as the file says: {list}");

    ok(&call(&mut ws, 4, "headend.channel.remove", json!({"names": ["CH2"]})).await);
    let list = shows(&mut ws, 5).await;
    assert!(input_of(&list, "ch2").is_null(), "hidden from the file, its show gone: {list}");
    assert!(std::fs::read_to_string(&conf).unwrap().contains("stream CH2"), "the file itself is not edited");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_headend_with_no_file_serves_its_own_channels_on_flussonics_urls() {
    let (dir, port) = folder("headend-nofile");
    let fport = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port().to_string();
    let st = start(dir.clone(), port, &["headend", "--port", &fport]).await;
    let mut ws = rpc(&st, "").await;
    let status = ok(&call(&mut ws, 1, "headend.status", json!({})).await).clone();
    assert_eq!(status["following"], false, "{status}");
    assert!(status["note"].as_str().unwrap().contains("headend.channel.add"), "{status}");
    ok(&call(&mut ws, 2, "headend.channel.add", json!({"channels": [{"name": "NEWS", "input": {"uri": "udp://127.0.0.1:9"}}]})).await);
    let (code, body) = facade(&fport, "/streamer/api/v3/streams").await;
    assert!(code == 200 && body.contains("\"NEWS\""), "{code} {body}");
    let (code, body) = facade(&fport, "/NEWS/index.m3u8").await;
    assert_ne!(code, 404, "a channel kept here is on the playback path too: {body}");
    let (code, _) = facade(&fport, "/NOPE/index.m3u8").await;
    assert_eq!(code, 404);
    let put = reqwest::Client::new().put(format!("http://127.0.0.1:{fport}/streamer/api/v3/streams/X")).body("{}").send().await.unwrap();
    assert_eq!(put.status().as_u16(), 409, "no file to edit");
    let _ = dir;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_headend_with_no_file_and_no_port_binds_nothing_more() {
    let (dir, port) = folder("headend-bare");
    let st = start(dir.clone(), port, &["headend"]).await;
    let mut ws = rpc(&st, "").await;
    let status = ok(&call(&mut ws, 1, "headend.status", json!({})).await).clone();
    assert!(status["listeners"].as_array().is_none_or(Vec::is_empty), "{status}");
}
