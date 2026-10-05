//! Clips made once a run, a scratch folder, and ffprobe reading back what
//! an input handed on.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use serde_json::Value;

use super::Collect;

/// Write what came out as FLV and ask ffprobe what it decodes. Answers
/// `(codec, frames, width, height, channels)` per stream.
pub fn probe(got: &Collect, name: &str) -> Vec<(String, u64, u64, u64, u64)> {
    let path = scratch().join(format!("{name}.flv"));
    let mut bytes = crate::flv::header();
    for tag in &got.0.lock().unwrap().tags {
        bytes.extend(crate::flv::write(tag));
    }
    std::fs::write(&path, bytes).unwrap();
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-count_frames", "-show_entries", "stream=codec_name,nb_read_frames,width,height,channels", "-of", "json"])
        .arg(&path)
        .output()
        .expect("ffprobe runs");
    let v: Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
    let n = |s: &Value, k: &str| s[k].as_u64().or_else(|| s[k].as_str().and_then(|t| t.parse().ok())).unwrap_or(0);
    v["streams"].as_array().cloned().unwrap_or_default().iter()
        .map(|s| (s["codec_name"].as_str().unwrap_or("").to_string(), n(s, "nb_read_frames"), n(s, "width"), n(s, "height"), n(s, "channels")))
        .collect()
}

pub fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gmx-directin-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

static CLIPS: Mutex<()> = Mutex::new(());
static SENDERS: Mutex<()> = Mutex::new(());

/// One live sender at a time: six encoders at once on a busy machine lose
/// datagrams in the kernel before the input sees them, which is the machine
/// and not the input.
pub fn one_at_a_time() -> std::sync::MutexGuard<'static, ()> {
    SENDERS.lock().unwrap_or_else(|e| e.into_inner())
}

/// A clip made once per test run with ffmpeg: `inputs` then `-t secs`, `out`.
pub fn clip(name: &str, args: &[&str]) -> PathBuf {
    let _one = CLIPS.lock().unwrap_or_else(|e| e.into_inner());
    let path = scratch().join(name);
    if !path.is_file() {
        let ok = Command::new("ffmpeg").args(["-y", "-loglevel", "error"]).args(args).arg(&path).status().unwrap().success();
        assert!(ok, "ffmpeg could not make {name}");
    }
    path
}

/// A 320x240 25 fps H.264 and AAC clip in MPEG-TS, `secs` long.
pub fn ts_clip(secs: u32) -> PathBuf {
    let t = secs.to_string();
    clip(&format!("h264-aac-{secs}.ts"), &["-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25", "-f", "lavfi", "-i", "sine=frequency=440",
        "-t", &t, "-c:v", "libx264", "-preset", "ultrafast", "-g", "25", "-c:a", "aac", "-f", "mpegts"])
}

pub fn path(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// A `file://` address for `p`. On Windows a drive path needs a third slash
/// and forward slashes, `file:///C:/Users/...`; written as `file://C:\...` it
/// is refused as no file address at all.
pub fn file_url(p: &Path) -> String {
    let s = path(p).replace('\\', "/");
    if s.starts_with('/') { format!("file://{s}") } else { format!("file:///{s}") }
}
