//! What feeds a channel, end to end through the station: the programme sent
//! into a channel and on to an RTMP server (ffmpeg listening), across a
//! restart of the station; and a channel pulling from an RTMP server that is
//! stopped and started again. Skipped where ffmpeg is not installed.

use super::direct_live::staged_ingest;
use super::hls_direct::folder_with_relay;
use super::support::*;
use serde_json::{json, Value};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub struct Ffmpeg(pub Child);

impl Drop for Ffmpeg {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn ffmpeg(args: &[&str]) -> Option<Ffmpeg> {
    let child = Command::new("ffmpeg").args(["-hide_banner", "-loglevel", "error"]).args(args).stdout(Stdio::null()).stderr(Stdio::null()).spawn().ok()?;
    std::thread::sleep(Duration::from_millis(500));
    Some(Ffmpeg(child))
}

pub fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// An RTMP server that writes what it is sent to `file`, as it came.
pub fn destination(port: u16, file: &Path) -> Option<Ffmpeg> {
    let url = format!("rtmp://127.0.0.1:{port}/live/dest");
    ffmpeg(&["-listen", "1", "-timeout", "180", "-i", &url, "-c", "copy", "-f", "flv", "-y", &file.to_string_lossy()])
}

/// An RTMP server a player pulls 320x180 H.264 and AAC from.
fn source(port: u16) -> Option<Ffmpeg> {
    let url = format!("rtmp://127.0.0.1:{port}/src/cam");
    ffmpeg(&[
        "-re", "-f", "lavfi", "-i", "testsrc2=size=320x180:rate=25", "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000",
        "-c:v", "libx264", "-preset", "ultrafast", "-tune", "zerolatency", "-g", "25", "-c:a", "aac", "-f", "flv", "-listen", "1", &url,
    ])
}

/// What ffprobe finds in a file: `codec_name,codec_type` for each stream.
fn probe(file: &Path) -> String {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "stream=codec_type,codec_name", "-of", "csv=p=0", &file.to_string_lossy()])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    out.replace("\r", "")
}

pub async fn channel(ws: &mut Ws, id: u64, channel: &str) -> Value {
    let got = call(ws, id, "channel.get", json!({"id": channel})).await;
    got["result"].clone()
}

/// Channel sunday once `ok` holds of it, within `limit`.
pub async fn until(ws: &mut Ws, what: &str, limit: Duration, ok: impl Fn(&Value) -> bool) -> Value {
    let (until, mut id) = (Instant::now() + limit, 500u64);
    loop {
        id += 1;
        let seen = channel(ws, id, "sunday").await;
        if ok(&seen) {
            return seen;
        }
        assert!(Instant::now() < until, "{what} did not happen within {} s: {seen}", limit.as_secs());
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// The governor turned the first destination away for want of CPU. A small CI
/// runner busy with other tests has no core to spare for an encode, and that
/// is the machine, not the code under test, so a test that converts skips.
pub fn refused_for_cpu(channel: &Value) -> bool {
    channel["destinations"][0]["refused"]["code"] == "governor"
}

pub async fn programme_outputs(ws: &mut Ws, id: u64) -> Vec<String> {
    let status = call(ws, id, "core.status", json!({})).await;
    let outputs = status["result"]["outputs"].as_array().cloned().unwrap_or_default();
    outputs.iter().filter_map(|o| o["id"].as_str().map(String::from)).filter(|o| o.starts_with("to-channel-")).collect()
}

pub async fn ready(dir: &Path, port: u16) -> (Running, Ws) {
    let st = start(dir.to_path_buf(), port, &[]).await;
    let ws = rpc(&st, "").await;
    (st, ws)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_programme_feeds_a_channel_whose_destination_gets_it_and_a_restart_adds_no_second_output() {
    if !godwinmix_core::probe::have_or_skip("rtmp2sink") {
        return;
    }
    let (dir, port) = folder_with_relay("feed-programme");
    let file = dir.join("dest.flv");
    let rtmp = free_port();
    let Some(_server) = destination(rtmp, &file) else {
        return eprintln!("ffmpeg is not installed; the feed test needs it as an RTMP server");
    };
    let plugin = staged_ingest(&dir);
    let (st, mut ws) = ready(&dir, port).await;
    let asked = call(&mut ws, 1, "plugin.add", json!({"source": plugin.to_string_lossy()})).await;
    assert!(asked.get("error").is_none(), "{asked}");
    tokio::time::sleep(Duration::from_secs(3)).await;
    let added = call(&mut ws, 2, "channel.add", json!({"name": "Sunday", "feed": {"kind": "programme"}})).await;
    assert_eq!(added["result"]["channel"]["feed"]["kind"], "programme", "{added}");
    assert_eq!(added["result"]["channel"]["feed"]["output"], "to-channel-sunday", "{added}");
    let dest = call(&mut ws, 3, "channel.destination.add", json!({"id": "sunday", "platform": "custom", "label": "Test",
        "server": format!("rtmp://127.0.0.1:{rtmp}/live"), "key": "dest"})).await;
    assert!(dest.get("error").is_none(), "{dest}");

    until(&mut ws, "the programme reaching the channel", Duration::from_secs(90), |c| c["feed"]["state"] == "live").await;
    let grown = |f: &Path| std::fs::metadata(f).map(|m| m.len()).unwrap_or(0) > 200_000;
    let since = Instant::now();
    while !grown(&file) {
        assert!(since.elapsed() < Duration::from_secs(90), "the destination got {} bytes", std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0));
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let found = probe(&file);
    assert!(found.contains("h264,video"), "the programme's own H.264 at the destination: {found}");
    assert!(found.contains("aac,audio"), "and its AAC: {found}");
    let live = channel(&mut ws, 4, "sunday").await;
    assert!(live["streams"][0]["source"].is_null(), "the programme is never made a source of itself: {live}");
    assert_eq!(programme_outputs(&mut ws, 5).await, vec!["to-channel-sunday"]);

    drop(ws);
    drop(st);
    let (_st, mut ws) = ready(&dir, port).await;
    tokio::time::sleep(Duration::from_secs(8)).await;
    assert_eq!(programme_outputs(&mut ws, 10).await, vec!["to-channel-sunday"], "kept by the show, and not added twice");
    let back = call(&mut ws, 11, "channel.set", json!({"id": "sunday", "feed": {"kind": "encoder"}})).await;
    assert_eq!(back["result"]["feed"]["kind"], "encoder", "{back}");
    let since = Instant::now();
    while !programme_outputs(&mut ws, 12).await.is_empty() {
        assert!(since.elapsed() < Duration::from_secs(30), "the programme's output did not go");
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pulled_channel_comes_back_by_itself_when_its_source_restarts() {
    if !godwinmix_core::probe::have_or_skip("flvdemux") {
        return;
    }
    let src = free_port();
    let Some(first) = source(src) else {
        return eprintln!("ffmpeg is not installed; the pull test needs it as an RTMP server");
    };
    let (dir, port) = folder_with_relay("feed-pull");
    let plugin = staged_ingest(&dir);
    let (_st, mut ws) = ready(&dir, port).await;
    let asked = call(&mut ws, 1, "plugin.add", json!({"source": plugin.to_string_lossy()})).await;
    assert!(asked.get("error").is_none(), "{asked}");
    tokio::time::sleep(Duration::from_secs(3)).await;
    let url = format!("rtmp://127.0.0.1:{src}/src/cam");
    let added = call(&mut ws, 2, "channel.add", json!({"name": "Sunday", "feed": {"kind": "pull", "url": url}})).await;
    assert_eq!(added["result"]["channel"]["feed"]["uri_host"], format!("rtmp://127.0.0.1:{src}"), "{added}");
    assert!(!added.to_string().contains("/src/cam"), "the pull address is never read back: {added}");

    let live = until(&mut ws, "the pull going live", Duration::from_secs(90), |c| c["feed"]["state"] == "live").await;
    assert_eq!(live["streams"][0]["protocol"], "pull", "{live}");
    assert_eq!(live["streams"][0]["video"]["codec"], "h264", "{live}");

    drop(first);
    let down = until(&mut ws, "the pull noticing its source went", Duration::from_secs(60), |c| c["feed"]["state"] != "live").await;
    assert!(!down.to_string().contains("/src/cam"), "nor in its error: {down}");
    let _second = source(src).expect("ffmpeg ran once already");
    let back = until(&mut ws, "the pull coming back", Duration::from_secs(90), |c| c["feed"]["state"] == "live").await;
    assert!(back["feed"]["error"].is_null(), "{back}");
}
