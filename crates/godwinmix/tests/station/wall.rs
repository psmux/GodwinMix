//! What the monitoring wall reads of the station besides the show methods:
//! a show's picture, what arrives on the machine, alarm settings, and a
//! missing method told from a missing show.

use super::support::*;
use serde_json::json;
use std::time::{Duration, Instant};

async fn fetch(st: &Running, path: &str) -> (u16, String, Vec<u8>) {
    let url = format!("http://{}{path}", st.url);
    let r = reqwest::Client::new().get(url).timeout(std::time::Duration::from_secs(20)).send().await.unwrap();
    let status = r.status().as_u16();
    let kind = r.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
    (status, kind, r.bytes().await.unwrap().to_vec())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_wall_gets_pictures_totals_and_alarm_settings_from_the_station() {
    let (dir, port) = folder("wall");
    let st = start(dir, port, &[]).await;
    let mut ws = rpc(&st, "").await;
    let added = call(&mut ws, 1, "show.add", json!({"name": "Feed", "compositing": false, "input": {"uri": "udp://@239.1.1.1:5000"}})).await;
    assert!(added.get("result").is_some(), "{added}");

    let (status, kind, body) = fetch(&st, "/api/v1/shows/main/thumbnail.jpg?width=160").await;
    assert_eq!((status, kind.as_str()), (200, "image/jpeg"), "a mixed show's programme: {}", String::from_utf8_lossy(&body));
    assert_eq!(&body[..2], &[0xff, 0xd8], "a JPEG");
    let (status, _, body) = fetch(&st, "/api/v1/shows/feed/thumbnail.jpg").await;
    assert_eq!(status, 409, "no host runs here, so no picture yet: {}", String::from_utf8_lossy(&body));
    assert!(String::from_utf8_lossy(&body).contains("retry_after_ms"));
    let (status, _, _) = fetch(&st, "/api/v1/shows/nope/thumbnail.jpg").await;
    assert_eq!(status, 404);

    // A task is read where plugin.add's answer says: the path reaches the
    // show, which says it has no such task, rather than no such route.
    let (status, _, body) = fetch(&st, "/api/v1/tasks/nope").await;
    assert_eq!(status, 404, "{}", String::from_utf8_lossy(&body));
    assert!(String::from_utf8_lossy(&body).contains("task"), "{}", String::from_utf8_lossy(&body));
    let (status, _, _) = fetch(&st, "/api/v1/tasks").await;
    assert_eq!(status, 200);

    let gov = call(&mut ws, 2, "governor.status", json!({})).await;
    assert!(gov["result"]["ingress_kbps"].is_u64(), "{gov}");

    let set = call(&mut ws, 3, "show.set", json!({"id": "feed", "alarms": {"black_ms": 2000, "enabled": true}})).await;
    assert_eq!(set["result"]["alarms"], json!({"enabled": true, "black_ms": 2000}), "{set}");
    let more = call(&mut ws, 4, "show.set", json!({"id": "feed", "alarms": {"silence_dbfs": -50.0}})).await;
    assert_eq!(more["result"]["alarms"]["black_ms"], 2000, "a field left out stays: {more}");
    assert_eq!(more["result"]["alarms"]["silence_dbfs"], -50.0);

    let missing = call(&mut ws, 5, "show.statz", json!({})).await;
    assert_eq!(missing["error"]["code"], -32601, "{missing}");
    let no_show = call(&mut ws, 6, "show.stats", json!({"ids": ["nope"]})).await;
    assert_ne!(no_show["error"]["code"], -32601, "{no_show}");
}

/// The wall's Load column and header for a station whose show that mixes
/// records its programme. That show holds no ticket (only renditions do), so
/// it reports nothing over the link and the station has to read its process.
/// Before, `show.stats` gave the wall no way to tell a show that mixes from
/// one that copies, and the header's CPU fell to the station's own few
/// millicores while the show's process ran.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_wall_counts_what_a_show_that_mixes_costs_in_its_load_and_header() {
    let (dir, port) = folder("wall-load");
    let st = start(dir.clone(), port, &[]).await;
    let mut ws = rpc(&st, "").await;
    let mut main = rpc(&st, "?show=main").await;
    let output = json!({"id": "archive", "uri": "record://programme", "type": "record/output",
        "params": {"format": "mkv", "directory": dir.join("recordings")}});
    let made = call(&mut main, 1, "output.add", output).await;
    assert!(made.get("result").is_some(), "{made}");
    let copy = json!({"name": "Copy", "compositing": false, "input": {"uri": "udp://@239.1.1.1:5000"}, "outputs": [{"id": "o", "uri": "udp://127.0.0.1:9"}]});
    let enc = json!({"name": "Enc", "compositing": false, "input": {"uri": "udp://@239.1.1.1:5002"},
        "outputs": [{"id": "o", "uri": "udp://127.0.0.1:9", "rendition": {"preset": "youtube-720p30"}}]});
    for (n, show) in [copy, enc].into_iter().enumerate() {
        let added = call(&mut ws, 2 + n as u64, "show.add", show).await;
        assert!(added.get("result").is_some(), "{added}");
    }

    let stats = call(&mut ws, 4, "show.stats", json!({})).await;
    let work = |id: &str| stats["result"]["shows"].as_array().unwrap().iter().find(|s| s["id"] == id).map(|s| s["work"].clone());
    assert_eq!(work("main"), Some(json!("mix")), "{stats}");
    assert_eq!(work("copy"), Some(json!("copy")), "{stats}");
    assert_eq!(work("enc"), Some(json!("transcode")), "{stats}");

    // A first start calibrates, which is the station's own work and swamps
    // everything else; its share then decays over a few seconds.
    let started = Instant::now();
    loop {
        let g = call(&mut ws, 5, "governor.status", json!({})).await;
        if g["result"]["calibrating"] == false && started.elapsed() > Duration::from_secs(25) {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(120), "still calibrating: {g}");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let s = call(&mut ws, 6, "show.stats", json!({"ids": ["main"]})).await;
    let mixes = s["result"]["shows"][0]["cpu_millicores"].as_u64().unwrap_or(0);
    let g = call(&mut ws, 7, "governor.status", json!({})).await;
    let used = g["result"]["cpu"]["measured_millicores"].as_u64().unwrap_or(0);
    assert!(mixes > 0, "a show recording its programme costs something: {s}");
    assert!(used >= mixes * 3 / 4, "the header's CPU, {used} millicores, leaves out the show that mixes at {mixes}: {g}");
}
