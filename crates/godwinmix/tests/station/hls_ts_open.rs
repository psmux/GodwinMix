//! MPEG-TS beside CMAF, and an output open to loopback, end to end through
//! the station: a real feed over UDP into the direct host, two HLS outputs
//! of one show. `open` serves both kinds of segment with no key at all;
//! `keyed`, with LL-HLS on, serves the same TS and its parts to a player
//! with its key. A TS segment starts with PAT and PMT, two in
//! a row are one clean stream, and GStreamer's discoverer decodes it.

use super::direct_live::{feed_with, free_udp, staged_ingest};
use super::hls_direct::{discover, folder_with_relay, text, until};
use super::support::*;
use gstreamer::prelude::*;
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;

async fn bytes(st: &Running, path: &str) -> Vec<u8> {
    let answer = reqwest::Client::new().get(format!("http://{}{path}", st.url)).timeout(Duration::from_secs(20)).send().await.unwrap();
    assert!(answer.status().is_success(), "{path}: {}", answer.status());
    answer.bytes().await.unwrap().to_vec()
}

/// The segment URIs of a media playlist.
fn uris(playlist: &str) -> Vec<String> {
    playlist.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_string).collect()
}

/// Whether every PID's continuity counter runs on through `ts`.
fn continuous(ts: &[u8]) -> bool {
    let mut last: HashMap<u16, u8> = HashMap::new();
    ts.chunks(188).all(|p| {
        let pid = (u16::from(p[1] & 0x1f) << 8) | u16::from(p[2]);
        let cc = p[3] & 0x0f;
        last.insert(pid, cc).is_none_or(|prev| cc == (prev + 1) & 0x0f)
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_open_output_serves_cmaf_and_ts_without_a_key_and_a_keyed_one_with_it() {
    if !godwinmix_core::probe::have_or_skip("cmafmux") {
        return;
    }
    let (dir, port) = folder_with_relay("hls-ts-open");
    let source = staged_ingest(&dir);
    let st = start(dir.clone(), port, &[]).await;
    let mut ws = rpc(&st, "").await;
    let _ = call(&mut ws, 1, "channel.remove", json!({"id": "live"})).await;
    let input = free_udp();
    let pipeline = feed_with(input, "avenc_aac ! aacparse");
    let outputs = json!([
        {"uri": "hls://open", "params": {"segment_ms": 1000, "window": 6, "segments": "both", "access": "open"}},
        {"uri": "hls://keyed", "params": {"segment_ms": 1000, "window": 6, "segments": "both", "low_latency": true}}
    ]);
    let added = call(&mut ws, 2, "show.add", json!({"name": "Feed", "compositing": false, "input": {"uri": format!("udp://127.0.0.1:{input}")}, "outputs": outputs})).await;
    assert!(added.get("error").is_none(), "{added}");
    let asked = call(&mut ws, 3, "plugin.add", json!({"source": source.to_string_lossy()})).await;
    assert!(asked.get("error").is_none(), "{asked}");
    until(&mut ws, "feed", "open", "live", Duration::from_secs(90)).await;
    let keyed = until(&mut ws, "feed", "keyed", "live", Duration::from_secs(90)).await;

    // Open: no key, no token, from loopback.
    let (status, cmaf) = text(&st, "/hls/open/master.m3u8?show=feed").await;
    assert_eq!(status, 200, "{cmaf}");
    let (status, master) = text(&st, "/hls/open/master-ts.m3u8?show=feed").await;
    assert_eq!(status, 200, "{master}");
    assert!(master.contains("avc1.") && master.contains("mp4a.40") && master.contains("TYPE=AUDIO"), "{master}");
    assert!(master.contains("main/ts.m3u8?show=feed"), "{master}");
    // The output is live with one whole CMAF segment; wait for two TS ones.
    let mut media = String::new();
    for _ in 0..40 {
        let (status, m) = text(&st, "/hls/open/main/ts.m3u8?show=feed").await;
        assert_eq!(status, 200, "{m}");
        media = m;
        if uris(&media).len() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert!(media.contains("#EXT-X-TARGETDURATION:") && media.contains("#EXT-X-PROGRAM-DATE-TIME:"), "{media}");
    let listed = uris(&media);
    assert!(listed.len() >= 2, "two whole segments: {media}");
    let (a, b) = (&listed[listed.len() - 2], &listed[listed.len() - 1]);
    assert!(a.contains(".ts?show=feed"), "{a}");
    let first = bytes(&st, &format!("/hls/open/main/{a}")).await;
    let second = bytes(&st, &format!("/hls/open/main/{b}")).await;
    assert_eq!((first[0], first.len() % 188), (0x47, 0), "whole packets");
    let pid = |p: &[u8]| (u16::from(p[1] & 0x1f) << 8) | u16::from(p[2]);
    assert_eq!((pid(&first[..188]), pid(&first[188..376]), pid(&first[376..])), (0, 0x1000, 0x100), "the PAT, the PMT, then the picture");
    let both = [first.clone(), second].concat();
    assert!(continuous(&both), "two segments in a row are one stream");
    let file = dir.join("two.ts");
    std::fs::write(&file, &both).unwrap();
    let seen = discover(&file);
    assert!(seen.contains("H.264") && !seen.contains("rror"), "the TS decodes: {seen}");

    // Keyed, with its key: the same TS, and CMAF parts too. (A station with
    // no control token lets everyone in, so the refusal without a key is
    // the unit test's in control::hls::auth.)
    let link = keyed["playback"]["master_url_path"].as_str().unwrap().to_string();
    let key = link.rsplit("key=").next().unwrap();
    let (status, master) = text(&st, &format!("/hls/keyed/master-ts.m3u8?show=feed&key={key}")).await;
    assert_eq!(status, 200, "{master}");
    assert!(master.contains(&format!("key={key}")), "the key rides on every URI: {master}");
    let (status, ll) = text(&st, &format!("/hls/keyed/main/index.m3u8?show=feed&key={key}")).await;
    assert_eq!(status, 200, "{ll}");
    assert!(ll.contains("#EXT-X-PART:") && ll.contains("CAN-BLOCK-RELOAD=YES"), "{ll}");
    let (status, ts) = text(&st, &format!("/hls/keyed/main/ts.m3u8?show=feed&key={key}")).await;
    assert_eq!(status, 200, "{ts}");
    assert!(!ts.contains("#EXT-X-PART"), "TS is plain HLS: {ts}");
    pipeline.set_state(gstreamer::State::Null).unwrap();
}
