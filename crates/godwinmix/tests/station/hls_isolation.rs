//! A crash in HLS packaging costs the HLS outputs and nothing else. Two
//! direct shows, one with an HLS output and one with a UDP copy, and the
//! HLS packager process killed outright, as a crash in GStreamer would end
//! it. The station's port keeps answering, the other show's copy keeps
//! flowing from the same direct host, the HLS output says it is
//! reconnecting and why, and then it comes back by itself on the same link.

use super::direct_live::{feed_with, free_udp, received, staged_ingest};
use super::hls_direct::{discover, fetch_rung, folder_with_relay, output, text, until};
use super::support::*;
use gstreamer::prelude::*;
use serde_json::json;
use std::net::UdpSocket;
use std::process::Command;
use std::time::{Duration, Instant};

/// A child of `parent` whose command line has `pattern` in it.
fn child_of(parent: u32, pattern: &str) -> Option<u32> {
    let out = Command::new("pgrep").args(["-P", &parent.to_string(), "-f", "--", pattern]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).lines().next()?.trim().parse().ok()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn killing_the_hls_packager_costs_the_hls_output_a_moment_and_nothing_else() {
    let (dir, port) = folder_with_relay("hls-isolation");
    let source = staged_ingest(&dir);
    let st = start(dir.clone(), port, &[]).await;
    let mut ws = rpc(&st, "").await;
    let _ = call(&mut ws, 1, "channel.remove", json!({"id": "live"})).await;
    let (hls_in, copy_in, receiver) = (free_udp(), free_udp(), UdpSocket::bind("127.0.0.1:0").unwrap());
    let (hls_feed, copy_feed) = (feed_with(hls_in, "avenc_aac ! aacparse"), feed_with(copy_in, "avenc_aac ! aacparse"));
    let added = call(&mut ws, 2, "show.add", json!({"name": "Feed", "compositing": false, "input": {"uri": format!("udp://127.0.0.1:{hls_in}")},
        "outputs": [{"uri": "hls://viewers", "params": {"segment_ms": 1000, "window": 6}}]})).await;
    assert!(added.get("error").is_none(), "{added}");
    let out = format!("udp://127.0.0.1:{}", receiver.local_addr().unwrap().port());
    let added = call(&mut ws, 3, "show.add", json!({"name": "Other", "compositing": false, "input": {"uri": format!("udp://127.0.0.1:{copy_in}")},
        "outputs": [{"id": "out", "uri": out}]})).await;
    assert!(added.get("error").is_none(), "{added}");
    let asked = call(&mut ws, 4, "plugin.add", json!({"source": source.to_string_lossy()})).await;
    assert!(asked.get("error").is_none(), "{asked}");

    let before = until(&mut ws, "feed", "viewers", "live", Duration::from_secs(90)).await;
    let master = before["playback"]["master_url_path"].as_str().unwrap().to_string();
    assert_eq!(text(&st, &master).await.0, 200);
    assert!(received(&receiver, Duration::from_secs(60), 100_000) >= 100_000, "the other show's copy never flowed");
    let packager = child_of(st.pid(), "--hls-packager").expect("the station runs the HLS packager as its child");
    let host = child_of(st.pid(), "gmx-ingest").expect("the station runs the direct host as its child");

    // As hard as a crash: no chance to say goodbye.
    unsafe { libc::kill(packager as i32, libc::SIGKILL) };
    let asked_at = Instant::now();
    let down = until(&mut ws, "feed", "viewers", "reconnecting", Duration::from_secs(10)).await;
    assert!(down["error"].as_str().unwrap_or_default().contains("HLS packager stopped"), "{down}");
    assert!(asked_at.elapsed() < Duration::from_secs(10), "the station's port answered all along");
    // The other show is served by the direct host, which never noticed.
    assert!(received(&receiver, Duration::from_secs(5), 50_000) >= 50_000, "the copy stopped while the packager was down");
    let other = output(&mut ws, 20, "other", "out").await;
    assert_eq!((other["state"].as_str(), other["reconnects"].as_u64()), (Some("live"), Some(0)), "{other}");
    assert_eq!(child_of(st.pid(), "gmx-ingest"), Some(host), "the direct host is the same process");

    // Back by itself, on the same link.
    let after = until(&mut ws, "feed", "viewers", "live", Duration::from_secs(90)).await;
    assert_eq!(after["playback"]["master_url_path"], before["playback"]["master_url_path"], "{after}");
    assert!(after["reconnects"].as_u64().unwrap_or(0) >= 1, "the restart is counted: {after}");
    let again = child_of(st.pid(), "--hls-packager").expect("a packager runs again");
    assert_ne!(again, packager, "a new process");
    let file = dir.join("after.mp4");
    fetch_rung(&st, &master, "main", &file).await;
    let seen = discover(&file);
    assert!(seen.contains("H.264") && !seen.contains("rror"), "a segment made after the restart decodes: {seen}");
    let health = call(&mut ws, 30, "show.stats", json!({"ids": ["other"]})).await;
    assert_eq!(health["result"]["shows"][0]["outputs"][0]["state"], "live", "{health}");

    // With no HLS output left there is no packager.
    let removed = call(&mut ws, 31, "show.output.remove", json!({"id": "feed", "output": "viewers"})).await;
    assert!(removed.get("error").is_none(), "{removed}");
    let gone_by = Instant::now() + Duration::from_secs(20);
    while child_of(st.pid(), "--hls-packager").is_some() {
        assert!(Instant::now() < gone_by, "the packager still runs with no HLS output to package");
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    hls_feed.set_state(gstreamer::State::Null).unwrap();
    copy_feed.set_state(gstreamer::State::Null).unwrap();
}
