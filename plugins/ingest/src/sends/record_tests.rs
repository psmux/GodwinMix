//! A channel's recording, run from the channel table against the hub: real
//! H.264 and AAC in, a file that decodes out, named for the channel, the
//! stream and the time, and a new file each time the stream goes live.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::*;
use crate::testfeed;

fn wait_for(what: &str, limit: Duration, mut ok: impl FnMut() -> bool) {
    let until = Instant::now() + limit;
    while !ok() {
        assert!(Instant::now() < until, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Publish `frames` frames of real video and sound, then leave.
fn go_live(hub: &Hub, frames: u32) {
    let tags = testfeed::tags(frames);
    let p = hub.publish("sunday", "main", "127.0.0.1:1", Some("k".into())).expect("publish");
    // Long enough for the destination to see the stream and read it.
    std::thread::sleep(Duration::from_millis(700));
    for t in tags {
        p.push(t);
        std::thread::sleep(Duration::from_millis(2));
    }
    std::thread::sleep(Duration::from_millis(300));
    drop(p);
}

fn file_row(sends: &Sends) -> Value {
    sends.rates().first().map(|r| r["file"].clone()).unwrap_or_default()
}

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gmx-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn address(dir: &Path) -> String {
    let d = dir.display().to_string().replace('\\', "/");
    format!("file:///{}/sunday-{{stream}}-{{time}}.ts", d.trim_start_matches('/'))
}

#[test]
fn a_recording_is_copied_to_a_file_named_for_the_stream_and_the_time_and_a_new_one_each_time_it_goes_live() {
    let dir = folder("record");
    let hub = Hub::new();
    let sends = Sends::new(hub.clone(), None);
    let w = Wanted {
        channel: "sunday".into(),
        app: "sunday".into(),
        id: "record".into(),
        platform: "file".into(),
        url: address(&dir),
        stream: "*".into(),
        feed: Feed::Copy,
    };
    sends.apply(vec![w], vec![]);
    go_live(&hub, 90);
    wait_for("the file to close", Duration::from_secs(10), || file_row(&sends)["open"] == false);

    let first = file_row(&sends);
    let name = first["name"].as_str().unwrap().to_string();
    assert!(name.starts_with("sunday-main-") && name.ends_with(".ts"), "{first}");
    let stamp = &name["sunday-main-".len()..name.len() - 3];
    assert_eq!((stamp.len(), &stamp[8..9]), (15, "-"), "yyyymmdd-hhmmss: {name}");
    let path = PathBuf::from(first["path"].as_str().unwrap());
    let size = std::fs::metadata(&path).unwrap().len();
    assert_eq!(first["bytes"].as_u64(), Some(size), "the size is what was written: {first}");
    assert!(first["duration_ms"].as_u64().unwrap() > 200, "{first}");
    let (pictures, sound) = testfeed::decode_ts(&path);
    assert!(pictures >= 80, "copied, every picture decodes: {pictures} of 90");
    assert!(sound > 0, "and the sound with it");

    // The encoder comes back: a second file, the first left as it was.
    go_live(&hub, 45);
    wait_for("the second file", Duration::from_secs(10), || {
        let f = file_row(&sends);
        f["open"] == false && f["name"].as_str() != Some(name.as_str())
    });
    let files = std::fs::read_dir(&dir).unwrap().count();
    assert_eq!(files, 2, "one file per time the stream went live");
    assert_eq!(std::fs::metadata(&path).unwrap().len(), size, "the first file is not written again");

    sends.apply(vec![], vec![]);
    let _ = std::fs::remove_dir_all(&dir);
}
