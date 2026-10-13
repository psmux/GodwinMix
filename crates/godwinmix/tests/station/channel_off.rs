//! A channel's destination switched off stops reaching its server, end to
//! end: the programme fed into a channel, sent on to an RTMP server (ffmpeg
//! listening and writing a file), then `channel.destination.set` with
//! `enabled: false`, as the switch on the destination's tile sends. Testers
//! of 0.3.7 switched a Custom RTMP destination off and read the stream as
//! still going. Skipped where ffmpeg is not installed.

use super::channel_feed::{channel, destination, free_port, programme_outputs, ready, until};
use super::direct_live::staged_ingest;
use super::hls_direct::folder_with_relay;
use super::support::*;
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, Instant};

fn size(f: &Path) -> u64 {
    std::fs::metadata(f).map(|m| m.len()).unwrap_or(0)
}

/// Sending, then switched off: the server's connection closes and nothing
/// more is written, while the programme still feeds the channel.
async fn switched_off_stops(name: &str, rendition: Option<Value>) {
    if !godwinmix_core::probe::have_or_skip("rtmp2sink") {
        return;
    }
    let (dir, port) = folder_with_relay(name);
    let file = dir.join("dest.flv");
    let rtmp = free_port();
    let Some(mut server) = destination(rtmp, &file) else {
        return eprintln!("ffmpeg is not installed; the test needs it as an RTMP server");
    };
    let plugin = staged_ingest(&dir);
    let (_st, mut ws) = ready(&dir, port).await;
    let asked = call(&mut ws, 1, "plugin.add", json!({"source": plugin.to_string_lossy()})).await;
    assert!(asked.get("error").is_none(), "{asked}");
    tokio::time::sleep(Duration::from_secs(3)).await;
    let added = call(&mut ws, 2, "channel.add", json!({"name": "Sunday", "feed": {"kind": "programme"}})).await;
    assert!(added.get("error").is_none(), "{added}");
    let mut params = json!({"id": "sunday", "platform": "custom", "label": "Custom RTMP",
        "server": format!("rtmp://127.0.0.1:{rtmp}/live"), "key": "dest"});
    if let Some(r) = rendition {
        params["rendition"] = r;
    }
    let dest = call(&mut ws, 3, "channel.destination.add", params).await;
    assert!(dest.get("error").is_none(), "{dest}");

    until(&mut ws, "the destination going live", Duration::from_secs(90), |c| c["destinations"][0]["state"] == "live").await;
    let since = Instant::now();
    while size(&file) < 100_000 {
        assert!(since.elapsed() < Duration::from_secs(90), "the destination got {} bytes", size(&file));
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    let off = call(&mut ws, 4, "channel.destination.set", json!({"id": "sunday", "destination": "custom", "enabled": false})).await;
    assert_eq!(off["result"]["destinations"][0]["state"], "off", "{off}");
    let asked_off = Instant::now();
    // ffmpeg ends when its publisher hangs up, so its exit is the server
    // seeing the connection close.
    while server.0.try_wait().unwrap().is_none() {
        assert!(asked_off.elapsed() < Duration::from_secs(10), "the server still had a connection 10 s after the switch went off ({} bytes)", size(&file));
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    eprintln!("{name}: the server saw the connection close {} ms after the switch went off", asked_off.elapsed().as_millis());
    let at_close = size(&file);
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(size(&file), at_close, "nothing written after the close");

    let seen = channel(&mut ws, 5, "sunday").await;
    assert_eq!(seen["destinations"][0]["state"], "off", "{seen}");
    assert_eq!(seen["destinations"][0]["kbps"].as_u64().unwrap_or(0), 0, "{seen}");
    assert_eq!(programme_outputs(&mut ws, 6).await, vec!["to-channel-sunday"], "the programme still feeds the channel itself");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copied_destination_switched_off_stops_reaching_its_server() {
    switched_off_stops("off-copy", None).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_converted_destination_switched_off_stops_reaching_its_server() {
    let custom = json!({"video": {"codec": "h264", "height": 180, "bitrate_kbps": 600}, "audio": {"codec": "aac", "bitrate_kbps": 128}});
    switched_off_stops("off-converted", Some(custom)).await;
}
