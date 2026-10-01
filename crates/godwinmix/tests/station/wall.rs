//! What the monitoring wall reads of the station besides the show methods:
//! a show's picture, what arrives on the machine, alarm settings, and a
//! missing method told from a missing show.

use super::support::*;
use serde_json::json;

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
