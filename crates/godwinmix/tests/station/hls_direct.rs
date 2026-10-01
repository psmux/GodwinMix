//! HLS from a show without compositing, end to end through the station: a
//! real MPEG-TS feed over UDP into the direct host, the station packaging
//! it, and plain HTTP fetching the playlists and segments from the
//! station's own port. A segment is handed to GStreamer's discoverer to
//! prove it decodes, and the playlist is read twice to prove it moves.

use super::direct_live::{feed_with, free_udp, staged_ingest};
use super::support::*;
use gstreamer::prelude::*;
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

async fn text(st: &Running, path: &str) -> (u16, String) {
    let answer = reqwest::Client::new().get(format!("http://{}{path}", st.url)).timeout(Duration::from_secs(20)).send().await.unwrap();
    (answer.status().as_u16(), answer.text().await.unwrap_or_default())
}

async fn bytes(st: &Running, path: &str) -> Vec<u8> {
    let answer = reqwest::Client::new().get(format!("http://{}{path}", st.url)).timeout(Duration::from_secs(20)).send().await.unwrap();
    assert!(answer.status().is_success(), "{path}: {}", answer.status());
    answer.bytes().await.unwrap().to_vec()
}

/// A folder whose ingest plugin binds its relay on a port of its own, so
/// two of these tests can run at once.
fn folder_with_relay(name: &str) -> (std::path::PathBuf, u16) {
    let (dir, port) = folder(name);
    let relay = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let config = dir.join("godwinmix.toml");
    let text = std::fs::read_to_string(&config).unwrap();
    std::fs::write(&config, format!("{text}\n[plugins.ingest]\nrtmp_port = {relay}\n")).unwrap();
    (dir, port)
}

/// The show's output as `show.list` has it.
async fn output(ws: &mut Ws, id: u64, show: &str, out: &str) -> Value {
    let list = call(ws, id, "show.list", json!({})).await;
    let shows = list["result"]["shows"].as_array().cloned().unwrap_or_default();
    let s = shows.into_iter().find(|s| s["id"] == show).unwrap_or_default();
    s["outputs"].as_array().and_then(|o| o.iter().find(|o| o["id"] == out).cloned()).unwrap_or_default()
}

/// Wait until the output is in `state`, and answer it.
async fn until(ws: &mut Ws, show: &str, out: &str, state: &str, limit: Duration) -> Value {
    let (until, mut id) = (Instant::now() + limit, 100);
    loop {
        id += 1;
        let o = output(ws, id, show, out).await;
        if o["state"] == state {
            return o;
        }
        if Instant::now() > until {
            let stats = call(ws, 99, "show.stats", json!({"ids": [show]})).await;
            panic!("{out} never became {state}: {o}\n{stats}");
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// The URIs a playlist lists, and its media sequence number.
fn listed(playlist: &str) -> (Vec<String>, u64) {
    let uris = playlist.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_string).collect();
    let seq = playlist.lines().find_map(|l| l.strip_prefix("#EXT-X-MEDIA-SEQUENCE:")).and_then(|n| n.trim().parse().ok()).unwrap_or(0);
    (uris, seq)
}

fn init_of(playlist: &str) -> String {
    let line = playlist.lines().find(|l| l.starts_with("#EXT-X-MAP:")).expect("a media playlist names its init segment");
    line.split("URI=\"").nth(1).and_then(|r| r.split('"').next()).unwrap().to_string()
}

/// What gst-discoverer-1.0 says of a file, with `-v` for the codecs.
fn discover(file: &Path) -> String {
    let out = Command::new("gst-discoverer-1.0").arg("-v").arg(file).output().expect("gst-discoverer-1.0 is installed with GStreamer");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

/// The newest whole segment of one rung, with its init segment in front,
/// written to `file`: a fragmented MP4 a player could play.
async fn fetch_rung(st: &Running, master: &str, rung: &str, file: &Path) -> (String, u64) {
    let base = "/hls/viewers/";
    let master_text = text(st, master).await.1;
    let uri = master_text.lines().find(|l| l.starts_with(&format!("{rung}/")) || l.contains(&format!("URI=\"{rung}/"))).unwrap_or_else(|| panic!("no {rung} in {master_text}"));
    let uri = uri.split("URI=\"").nth(1).and_then(|r| r.split('"').next()).unwrap_or(uri).to_string();
    let (status, playlist) = text(st, &format!("{base}{uri}")).await;
    assert_eq!(status, 200, "{playlist}");
    let (segments, seq) = listed(&playlist);
    let dir = format!("{base}{rung}/");
    let mut body = bytes(st, &format!("{dir}{}", init_of(&playlist))).await;
    body.extend(bytes(st, &format!("{dir}{}", segments.last().expect("a segment"))).await);
    std::fs::write(file, body).unwrap();
    (playlist, seq)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_direct_show_serves_its_feed_as_hls_from_the_station_copied_not_decoded() {
    let (dir, port) = folder_with_relay("hls-direct");
    let source = staged_ingest(&dir);
    let st = start(dir.clone(), port, &[]).await;
    let mut ws = rpc(&st, "").await;
    let _ = call(&mut ws, 1, "channel.remove", json!({"id": "live"})).await;
    let input = free_udp();
    let pipeline = feed_with(input, "avenc_aac ! aacparse");
    let added = call(&mut ws, 2, "show.add", json!({"name": "Feed", "compositing": false, "input": {"uri": format!("udp://127.0.0.1:{input}")},
        "outputs": [{"uri": "hls://viewers", "params": {"segment_ms": 1000, "window": 6}}]})).await;
    assert!(added.get("error").is_none(), "{added}");
    let asked = call(&mut ws, 3, "plugin.add", json!({"source": source.to_string_lossy()})).await;
    assert!(asked.get("error").is_none(), "{asked}");

    let out = until(&mut ws, "feed", "viewers", "live", Duration::from_secs(90)).await;
    let master = out["playback"]["master_url_path"].as_str().expect("an HLS output says where to play it").to_string();
    assert!(master.starts_with("/hls/viewers/master.m3u8?show=feed&key="), "{out}");

    let (status, text_master) = text(&st, &master).await;
    assert_eq!(status, 200, "{text_master}");
    assert!(text_master.contains("avc1.") && text_master.contains("mp4a."), "both tracks, copied: {text_master}");
    assert!(text_master.contains("show=feed"), "every URI carries the show: {text_master}");
    let (status, missing) = text(&st, "/hls/nobody/master.m3u8?show=feed").await;
    assert_eq!(status, 404, "{missing}");
    assert!(missing.contains("It serves viewers"), "a wrong name is told the right one: {missing}");

    let file = dir.join("main.mp4");
    let (first, _) = fetch_rung(&st, &master, "main", &file).await;
    let seen = discover(&file);
    assert!(seen.contains("H.264") && !seen.contains("rror"), "the video segment decodes: {seen}");
    let file = dir.join("audio.mp4");
    fetch_rung(&st, &master, "audio", &file).await;
    let seen = discover(&file);
    assert!(seen.contains("AAC") || seen.contains("MPEG-4 AAC"), "the sound segment decodes: {seen}");

    tokio::time::sleep(Duration::from_secs(3)).await;
    let (later, _) = fetch_rung(&st, &master, "main", &dir.join("main2.mp4")).await;
    let (a, b) = (listed(&first), listed(&later));
    assert!(b.0.last() != a.0.last(), "the playlist moved on in three seconds:\n{first}\n{later}");
    let stats = call(&mut ws, 50, "show.stats", json!({"ids": ["feed"]})).await;
    assert_eq!(stats["result"]["shows"][0]["outputs"][0]["state"], "live", "{stats}");
    assert_eq!(stats["result"]["shows"][0]["work"], "copy", "{stats}");
    pipeline.set_state(gstreamer::State::Null).unwrap();
}

/// MPEG audio layer II, as many broadcast feeds carry: copied, it would make
/// HLS no player plays, so the output says so and names the rendition that
/// fixes it. Given that rendition, the picture is still copied and the
/// sound comes out as AAC.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mp2_sound_is_refused_with_the_next_step_and_served_once_a_rendition_makes_aac() {
    let (dir, port) = folder_with_relay("hls-direct-mp2");
    let source = staged_ingest(&dir);
    let st = start(dir.clone(), port, &[]).await;
    let mut ws = rpc(&st, "").await;
    let _ = call(&mut ws, 1, "channel.remove", json!({"id": "live"})).await;
    let input = free_udp();
    let pipeline = feed_with(input, "avenc_mp2 ! mpegaudioparse");
    let added = call(&mut ws, 2, "show.add", json!({"name": "Feed", "compositing": false, "input": {"uri": format!("udp://127.0.0.1:{input}")},
        "outputs": [{"uri": "hls://viewers", "params": {"segment_ms": 1000, "window": 6}}]})).await;
    assert!(added.get("error").is_none(), "{added}");
    let asked = call(&mut ws, 3, "plugin.add", json!({"source": source.to_string_lossy()})).await;
    assert!(asked.get("error").is_none(), "{asked}");

    let out = until(&mut ws, "feed", "viewers", "failed", Duration::from_secs(90)).await;
    let why = out["error"].as_str().unwrap_or_default();
    assert!(why.contains("mp2") && why.contains("AAC") && why.contains("show.output.set"), "{out}");
    // Now that the input's sound is known, a second copy is refused at once.
    let again = call(&mut ws, 5, "show.output.add", json!({"id": "feed", "uri": "hls://second"})).await;
    assert_eq!(again["error"]["data"]["rendition"], json!({"audio": {"codec": "aac"}}), "{again}");
    assert_eq!(again["error"]["data"]["audio_codec"], "mp2", "{again}");

    let set = call(&mut ws, 4, "show.output.set", json!({"id": "feed", "output": "viewers", "rendition": {"audio": {"codec": "aac"}}})).await;
    assert!(set.get("error").is_none(), "{set}");
    // The sound decode and encode are admitted by the governor, which
    // waits for room when other tests have the machine busy.
    let out = until(&mut ws, "feed", "viewers", "live", Duration::from_secs(240)).await;
    let master = out["playback"]["master_url_path"].as_str().unwrap().to_string();
    let (_, text_master) = text(&st, &master).await;
    assert!(text_master.contains("avc1.") && text_master.contains("mp4a.40"), "{text_master}");
    let file = dir.join("audio.mp4");
    fetch_rung(&st, &master, "audio", &file).await;
    let seen = discover(&file);
    assert!(seen.contains("AAC") && !seen.contains("rror"), "the sound segment decodes as AAC: {seen}");
    let file = dir.join("main.mp4");
    fetch_rung(&st, &master, "main", &file).await;
    let seen = discover(&file);
    assert!(seen.contains("H.264") && !seen.contains("rror"), "{seen}");
    pipeline.set_state(gstreamer::State::Null).unwrap();
}
