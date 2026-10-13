//! A channel's destination switched off stops reaching its server, end to
//! end: the programme fed into a channel, sent on to an RTMP server (ffmpeg
//! listening and writing a file), then `channel.destination.set` with
//! `enabled: false`, as the switch on the destination's tile sends. Testers
//! of 0.3.7 switched a Custom RTMP destination off and read the stream as
//! still going. Skipped where ffmpeg is not installed.

use super::channel_feed::{channel, destination, free_port, programme_outputs, ready, refused_for_cpu};
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

    let id = dest["result"]["destinations"][0]["id"].as_str().unwrap().to_string();

    // ffmpeg takes one connection and ends with it. A converted destination
    // may hang up once while its plan settles, so the server is started
    // again until a connection stays and the file grows.
    let (since, mut restarts) = (Instant::now(), 0);
    while size(&file) < 100_000 || server.0.try_wait().unwrap().is_some() {
        assert!(since.elapsed() < Duration::from_secs(120), "the destination got {} bytes after {restarts} restarts", size(&file));
        let seen = channel(&mut ws, 900 + restarts as u64, "sunday").await;
        if refused_for_cpu(&seen) {
            return eprintln!("{name}: skipping, the governor has no core free for the encode on this machine: {}", seen["destinations"][0]["error"]);
        }
        if server.0.try_wait().unwrap().is_some() {
            server = destination(rtmp, &file).expect("ffmpeg ran once already");
            restarts += 1;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    eprintln!("{name}: {} bytes reached the server, which was started again {restarts} times", size(&file));
    let live = channel(&mut ws, 4, "sunday").await;
    assert_eq!(live["destinations"][0]["state"], "live", "{live}");

    let off = call(&mut ws, 5, "channel.destination.set", json!({"id": "sunday", "destination": id, "enabled": false})).await;
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

    let seen = channel(&mut ws, 6, "sunday").await;
    assert_eq!(seen["destinations"][0]["state"], "off", "{seen}");
    assert_eq!(seen["destinations"][0]["kbps"].as_u64().unwrap_or(0), 0, "{seen}");
    assert_eq!(programme_outputs(&mut ws, 7).await, vec!["to-channel-sunday"], "the programme still feeds the channel itself");
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
