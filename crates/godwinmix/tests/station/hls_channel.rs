//! A channel's Record and Watch link, end to end through the station: a
//! real encoder publishing RTMP into a channel, the listener writing the
//! recording, the station's packager serving the watch link, and plain HTTP
//! with no token fetching the playlists and segments. GStreamer's
//! discoverer proves the segment and the recording both decode.

use super::direct_live::staged_ingest;
use super::hls_direct::{discover, folder_with_relay, text};
use super::support::*;
use gstreamer::prelude::*;
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, Instant};

/// An encoder publishing 320x180 H.264 and AAC to `url`, as OBS would.
fn encoder(url: &str) -> gstreamer::Pipeline {
    gstreamer::init().unwrap();
    let line = format!(
        "videotestsrc is-live=true ! video/x-raw,width=320,height=180,framerate=30/1 ! x264enc tune=zerolatency key-int-max=30 bitrate=600 \
         ! h264parse ! flvmux streamable=true name=m ! rtmp2sink location=\"{url}\" \
         audiotestsrc is-live=true ! audioconvert ! audioresample ! audio/x-raw,rate=48000,channels=2 ! avenc_aac ! aacparse ! m."
    );
    let p = gstreamer::parse::launch(&line).expect("x264enc, avenc_aac, flvmux and rtmp2sink are installed").downcast::<gstreamer::Pipeline>().unwrap();
    p.set_state(gstreamer::State::Playing).unwrap();
    p
}

/// One destination of the channel as `channel.get` has it.
async fn destination(ws: &mut Ws, id: u64, dest: &str) -> Value {
    let got = call(ws, id, "channel.get", json!({"id": "sunday"})).await;
    let list = got["result"]["destinations"].as_array().cloned().unwrap_or_default();
    list.into_iter().find(|d| d["id"] == dest).unwrap_or_default()
}

async fn until(ws: &mut Ws, dest: &str, ok: impl Fn(&Value) -> bool, limit: Duration) -> Value {
    let (until, mut id) = (Instant::now() + limit, 200);
    loop {
        id += 1;
        let d = destination(ws, id, dest).await;
        if ok(&d) {
            return d;
        }
        assert!(Instant::now() < until, "{dest} never got there: {d}");
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn fetch(st: &Running, path: &str) -> Vec<u8> {
    let answer = reqwest::Client::new().get(format!("http://{}{path}", st.url)).timeout(Duration::from_secs(20)).send().await.unwrap();
    assert!(answer.status().is_success(), "{path}: {}", answer.status());
    answer.bytes().await.unwrap().to_vec()
}

/// The video rung's init segment and newest segment, as one playable file.
async fn video_segment(st: &Running, master: &str, file: &Path) -> String {
    let base = master.split('?').next().unwrap().trim_end_matches("index.m3u8").to_string();
    let (_, top) = text(st, master).await;
    let rung = top.lines().find(|l| !l.starts_with('#') && !l.is_empty()).unwrap_or_else(|| panic!("no rung in {top}"));
    let (status, playlist) = text(st, &format!("{base}{rung}")).await;
    assert_eq!(status, 200, "{playlist}");
    let dir = format!("{base}{}", &rung[..=rung.find('/').unwrap()]);
    let map = playlist.lines().find_map(|l| l.split("URI=\"").nth(1)).and_then(|r| r.split('"').next()).expect("an init segment");
    let last = playlist.lines().rev().find(|l| !l.starts_with('#') && !l.is_empty()).expect("a segment");
    let mut body = fetch(st, &format!("{dir}{map}")).await;
    body.extend(fetch(st, &format!("{dir}{last}")).await);
    std::fs::write(file, body).unwrap();
    playlist
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_channel_records_to_a_file_and_serves_a_watch_link_both_copied() {
    if !godwinmix_core::probe::have_or_skip("cmafmux") || !godwinmix_core::probe::have_or_skip("rtmp2sink") {
        return;
    }
    let (dir, port) = folder_with_relay("hls-channel");
    let rtmp = std::fs::read_to_string(dir.join("godwinmix.toml")).unwrap();
    let rtmp: u16 = rtmp.rsplit("rtmp_port = ").next().unwrap().trim().parse().unwrap();
    let source = staged_ingest(&dir);
    let st = start(dir.clone(), port, &[]).await;
    let mut ws = rpc(&st, "").await;
    let added = call(&mut ws, 1, "channel.add", json!({"name": "Sunday"})).await;
    let secret = added["result"]["key"]["secret"].as_str().unwrap_or_else(|| panic!("{added}")).to_string();
    let rec = dir.join("recordings");
    let record = call(&mut ws, 2, "channel.destination.add", json!({"id": "sunday", "platform": "file", "server": rec.to_string_lossy()})).await;
    assert!(record.get("error").is_none(), "{record}");
    let watch = call(&mut ws, 3, "channel.destination.add", json!({"id": "sunday", "platform": "hls", "server": "hls://?segment_ms=1000&window=6"})).await;
    assert!(watch.get("error").is_none(), "{watch}");
    let asked = call(&mut ws, 4, "plugin.add", json!({"source": source.to_string_lossy()})).await;
    assert!(asked.get("error").is_none(), "{asked}");
    // The listener takes a publisher once the plugin is up.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let pipeline = encoder(&format!("rtmp://127.0.0.1:{rtmp}/sunday/main?psk={secret}"));

    let link = until(&mut ws, "watch-link", |d| d["state"] == "live", Duration::from_secs(90)).await;
    let master = link["playback"]["master_url_path"].as_str().expect("a watch link says where to play it").to_string();
    assert!(master.starts_with("/hls/channel/sunday/watch-link/index.m3u8?key="), "{link}");
    let (status, top) = text(&st, &master).await;
    // This station has no token, so the port is open anyway; the key is
    // what a station with one lets a player in by (`auth::admit`).
    assert_eq!(status, 200, "no token, only the link's key: {top}");
    assert!(top.contains("avc1.") && top.contains("mp4a."), "both tracks, copied: {top}");
    let (status, missing) = text(&st, "/hls/channel/sunday/nobody/index.m3u8").await;
    assert_eq!(status, 404, "{missing}");
    assert!(missing.contains("It has watch-link"), "a wrong name is told the right one: {missing}");
    let segment = dir.join("watch.mp4");
    let playlist = video_segment(&st, &master, &segment).await;
    let seen = discover(&segment);
    assert!(seen.contains("H.264") && !seen.contains("rror"), "the segment decodes: {seen}\n{playlist}");

    let recording = until(&mut ws, "record", |d| d["file"]["bytes"].as_u64().unwrap_or(0) > 100_000, Duration::from_secs(60)).await;
    let name = recording["file"]["name"].as_str().unwrap().to_string();
    assert!(name.starts_with("sunday-main-") && name.ends_with(".ts"), "{recording}");
    pipeline.set_state(gstreamer::State::Null).unwrap();
    let closed = until(&mut ws, "record", |d| d["file"]["open"] == false, Duration::from_secs(30)).await;
    let path = closed["file"]["path"].as_str().unwrap().to_string();
    assert!(Path::new(&path).starts_with(&rec), "{closed}");
    let seen = discover(Path::new(&path));
    assert!(seen.contains("H.264") && (seen.contains("AAC") || seen.contains("MPEG-4")), "the recording decodes: {seen}");

    let off = call(&mut ws, 5, "channel.destination.set", json!({"id": "sunday", "destination": "watch-link", "enabled": false})).await;
    assert!(off.get("error").is_none(), "{off}");
    tokio::time::sleep(Duration::from_secs(2)).await;
    let (status, gone) = text(&st, &master).await;
    assert_eq!(status, 404, "a link switched off serves nothing: {gone}");
}
