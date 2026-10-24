//! A direct show's HLS output when another program holds the ingest
//! plugin's RTMP port on loopback, as a GodwinMix or an ingest plugin left
//! running from an earlier start does. The relay the packager reads from
//! must be one that reaches this station's plugin, and the output goes
//! live. Before, on Windows the plugin bound the port on every interface
//! beside the other program, took 127.0.0.1:<port> for its relay, and the
//! packager read nothing from it for ever. Here, on Linux, the bind on
//! every interface is refused; either way the relay moves to a port of
//! its own.

use super::direct_live::{feed_with, free_udp, staged_ingest};
use super::hls_direct::{folder_with_relay, text, until};
use super::support::*;
use gstreamer::prelude::*;
use serde_json::json;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_direct_hls_output_goes_live_when_another_program_holds_the_rtmp_port_on_loopback() {
    if !godwinmix_core::probe::have_or_skip("cmafmux") {
        return;
    }
    let (dir, port) = folder_with_relay("hls-relay-taken");
    let config = std::fs::read_to_string(dir.join("godwinmix.toml")).unwrap();
    let rtmp: u16 = config.rsplit("rtmp_port = ").next().and_then(|r| r.trim().parse().ok()).expect("folder_with_relay wrote the port");
    // The other program: it holds the port on loopback and answers nobody.
    let other = std::net::TcpListener::bind(("127.0.0.1", rtmp)).expect("the port is free for the other program");
    let source = staged_ingest(&dir);
    let st = start(dir.clone(), port, &[]).await;
    let mut ws = rpc(&st, "").await;
    let input = free_udp();
    let pipeline = feed_with(input, "avenc_aac ! aacparse");
    let added = call(&mut ws, 2, "show.add", json!({"name": "CH101", "compositing": false, "input": {"uri": format!("udp://127.0.0.1:{input}")},
        "outputs": [{"uri": "hls://flussonic", "params": {"segment_ms": 1000, "window": 6, "segments": "both", "access": "open"}}]})).await;
    assert!(added.get("error").is_none(), "{added}");
    let asked = call(&mut ws, 3, "plugin.add", json!({"source": source.to_string_lossy()})).await;
    assert!(asked.get("error").is_none(), "{asked}");

    let out = until(&mut ws, "ch101", "flussonic", "live", Duration::from_secs(90)).await;
    assert_eq!(out["state"], "live", "{out}");
    let (status, master) = text(&st, "/hls/flussonic/master.m3u8?show=ch101").await;
    assert_eq!(status, 200, "{master}");
    // A path that is not a playlist names the ones that are.
    let (status, wrong) = text(&st, "/hls/flussonic/index.m3u8?show=ch101").await;
    assert_eq!(status, 404, "{wrong}");
    assert!(wrong.contains("/hls/flussonic/master.m3u8?show=ch101") && !wrong.contains("compositing"), "{wrong}");
    pipeline.set_state(gstreamer::State::Null).unwrap();
    drop(other);
}
