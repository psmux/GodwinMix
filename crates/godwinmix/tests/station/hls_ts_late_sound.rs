//! MPEG-TS from a converted channel's timing, through the HLS packager the
//! station runs: a keyframe every 1668 ms and the sound 0.7 s behind the
//! picture on the relay. The TS segments must be cut where the CMAF ones
//! are, every 1.668 s. Before the cutter measured its wait from the newest
//! time queued, the late sound made every waiting keyframe look stale, and
//! TS made one segment of a minute where CMAF made forty.

use super::late_sound::relay;
use super::support::BIN;
use serde_json::json;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SECRET: &str = "late-sound-test-secret";

/// EXTINF of every segment a media playlist lists.
fn lengths(playlist: &str) -> Vec<f64> {
    playlist.lines().filter_map(|l| l.strip_prefix("#EXTINF:")).filter_map(|l| l.trim_end_matches(',').parse().ok()).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ts_is_cut_every_keyframe_when_the_sound_comes_late() {
    if !godwinmix_core::probe::have_or_skip("cmafmux") || !godwinmix_core::probe::have_or_skip("x264enc") || !godwinmix_core::probe::have_or_skip("avenc_aac") {
        return;
    }
    let source = relay(24);
    let log = std::env::temp_dir().join(format!("gmx-late-sound-{}.log", std::process::id()));
    let mut packager = Command::new(BIN)
        .arg("--hls-packager")
        .env("GODWINMIX_PACKAGER_SECRET", SECRET)
        .env("GODWINMIX_HOME", std::env::temp_dir().join(format!("gmx-late-sound-{}", std::process::id())))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(&log).unwrap())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(packager.stdout.take().unwrap()).read_line(&mut line).unwrap();
    let addr = line.trim().rsplit(' ').next().unwrap().to_string();
    let http = reqwest::Client::new();
    let want = json!([{ "show": "feed", "output": "tv", "source": { "relay": source.to_string(), "path": "direct.feed/main" },
        "why_not": null, "segment_ms": 1000, "part_ms": 0, "window_s": 60, "segments": "both", "access": "keyed",
        "viewer_key": "k".repeat(24), "sounds": [{ "track": 0, "lang": "eng" }] }]);
    let put = http.put(format!("http://{addr}/packager/outputs")).bearer_auth(SECRET).json(&want).send().await.unwrap();
    assert!(put.status().is_success(), "{}", put.status());
    let get = |file: &'static str| {
        let (http, addr) = (http.clone(), addr.clone());
        async move {
            let r = http.get(format!("http://{addr}/hls/tv/main/{file}")).bearer_auth(SECRET).header("x-godwinmix-show", "feed").send().await.unwrap();
            r.text().await.unwrap_or_default()
        }
    };
    let until = Instant::now() + Duration::from_secs(40);
    let (mut ts, mut cmaf) = (Vec::new(), Vec::new());
    while Instant::now() < until && ts.len() < 8 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        ts = lengths(&get("ts.m3u8").await);
        cmaf = lengths(&get("index.m3u8").await);
    }
    let report = http.get(format!("http://{addr}/packager/outputs")).bearer_auth(SECRET).send().await.unwrap().text().await.unwrap_or_default();
    let _ = packager.kill();
    let _ = packager.wait();
    let said = std::fs::read_to_string(&log).unwrap_or_default();
    let _ = std::fs::remove_file(&log);
    assert!(!cmaf.is_empty(), "the packager made nothing: {report}\n{}", said.lines().rev().take(15).collect::<Vec<_>>().join("\n"));
    assert!(cmaf.len() >= 8 && cmaf.iter().all(|d| (d - 1.668).abs() < 0.05), "CMAF is cut every keyframe: {cmaf:?}");
    assert!(ts.len() >= 8, "TS has a segment per keyframe too: {ts:?}, CMAF {cmaf:?}");
    assert!(ts.iter().all(|d| (d - 1.668).abs() < 0.05), "and each one is 1.668 s: {ts:?}");
}
