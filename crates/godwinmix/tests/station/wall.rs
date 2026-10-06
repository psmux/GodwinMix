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
    // Two minutes, times GODWINMIX_TIMING_SLACK: calibration encodes with
    // every encoder this machine has, and a three core macOS runner busy
    // with the rest of the suite was still at it after two.
    let started = Instant::now();
    let limit = Duration::from_secs(120).mul_f64(godwinmix_core::plugin::harness::timing_slack());
    loop {
        let g = call(&mut ws, 5, "governor.status", json!({})).await;
        if g["result"]["calibrating"] == false && started.elapsed() > Duration::from_secs(25) {
            break;
        }
        assert!(started.elapsed() < limit, "still calibrating after {limit:?}: {g}");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let s = call(&mut ws, 6, "show.stats", json!({"ids": ["main"]})).await;
    let mixes = s["result"]["shows"][0]["cpu_millicores"].as_u64().unwrap_or(0);
    let g = call(&mut ws, 7, "governor.status", json!({})).await;
    let used = g["result"]["cpu"]["measured_millicores"].as_u64().unwrap_or(0);
    assert!(mixes > 0, "a show recording its programme costs something: {s}");
    assert!(used >= mixes * 3 / 4, "the header's CPU, {used} millicores, leaves out the show that mixes at {mixes}: {g}");
}

/// The width and height a JPEG's frame header says, or None.
fn jpeg_size(b: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2;
    while i + 9 < b.len() {
        if b[i] != 0xff {
            return None;
        }
        let marker = b[i + 1];
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if matches!(marker, 0xc0..=0xc2) {
            let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
            let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
            return Some((w, h));
        }
        i += 2 + len;
    }
    None
}

/// One gauge's value from a show's `/metrics`, read through the station.
async fn gauge(st: &Running, line: &str) -> Option<f64> {
    let (_, _, body) = fetch(st, "/metrics?show=main").await;
    let text = String::from_utf8_lossy(&body).to_string();
    text.lines().find_map(|l| l.strip_prefix(line).map(|v| v.trim().parse().unwrap_or(-1.0)))
}

/// A show that composites gets a real picture of its programme on the wall,
/// at the wall's width, new each time, from `program.thumbnail` rather than a
/// mosaic, and the branch that makes it goes once nobody asks. Before, the
/// station asked for the programme cell of the show's mosaic, which built a
/// mosaic for one small picture and was refused two times in three by the
/// snapshot rate limit at the wall's two second cadence.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_show_that_mixes_has_a_moving_picture_on_the_wall_only_while_asked() {
    let (dir, port) = folder("wall-mixed-pic");
    let st = start(dir, port, &[]).await;
    let mut main = rpc(&st, "?show=main").await;
    let added = call(&mut main, 1, "source.add", json!({"id": "ball", "uri": "test://ball"})).await;
    assert!(added.get("result").is_some(), "{added}");
    let took = call(&mut main, 2, "program.take", json!({"source": "ball"})).await;
    assert!(took.get("result").is_some(), "{took}");
    let off = "gmx_stream_clients{kind=\"thumbnail\"}";
    assert_eq!(gauge(&st, off).await, Some(0.0), "nothing runs before the wall asks");

    let started = Instant::now();
    let first = loop {
        let (status, kind, body) = fetch(&st, "/api/v1/shows/main/thumbnail.jpg?width=160").await;
        if status == 200 {
            assert_eq!(kind, "image/jpeg");
            break body;
        }
        assert_eq!(status, 409, "{}", String::from_utf8_lossy(&body));
        assert!(started.elapsed() < Duration::from_secs(30), "no picture in 30 s: {}", String::from_utf8_lossy(&body));
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    assert_eq!(&first[..2], &[0xff, 0xd8], "a JPEG");
    assert_eq!(jpeg_size(&first), Some((160, 90)), "the wall's width, the canvas's shape");
    assert_eq!(gauge(&st, off).await, Some(1.0), "the branch is on while asked");
    assert_eq!(gauge(&st, "gmx_multiview_subscribers").await, Some(0.0), "and no mosaic was built for it");

    let changed = Instant::now();
    loop {
        tokio::time::sleep(Duration::from_millis(1100)).await;
        let (status, _, body) = fetch(&st, "/api/v1/shows/main/thumbnail.jpg?width=320").await;
        if status == 200 {
            assert_eq!(jpeg_size(&body), Some((320, 180)), "a tile's width");
        }
        let (status, _, small) = fetch(&st, "/api/v1/shows/main/thumbnail.jpg?width=160").await;
        if status == 200 && small != first {
            break;
        }
        assert!(changed.elapsed() < Duration::from_secs(15), "the picture did not change in 15 s");
    }

    let idle = Instant::now();
    while gauge(&st, off).await != Some(0.0) {
        assert!(idle.elapsed() < Duration::from_secs(25), "the thumbnail branch kept running with nobody asking");
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert!(idle.elapsed() >= Duration::from_secs(5), "it went before the ten seconds an ask buys");
}
