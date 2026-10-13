//! A Custom format on a channel fed by the programme reaches the far end as
//! asked. Testers set Custom RTMP to H.264 1280x720 at 30 fps, 3000 kb/s,
//! keyframe 2 s and AAC 128 kb/s under a channel fed at 1920x1080 and 6 Mb/s,
//! and could not tell whether the 720p went out. Here an RTMP server (ffmpeg
//! listening) receives it and ffprobe reads the size, the rate, the
//! keyframes and the bitrate of what arrived. Skipped where ffmpeg is not
//! installed.

use super::channel_feed::{channel, destination, free_port, ready, until};
use super::direct_live::staged_ingest;
use super::hls_direct::folder_with_relay;
use super::support::call;
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

/// What testers chose under Custom.
fn custom_720p() -> Value {
    json!({"container": "flv", "video": {"codec": "h264", "width": 1280, "height": 720, "fps": {"num": 30, "den": 1},
        "bitrate_kbps": 3000, "keyframe_ms": 2000}, "audio": {"codec": "aac", "bitrate_kbps": 128}})
}

fn ffprobe(args: &[&str], file: &Path) -> String {
    let out = Command::new("ffprobe").args(["-v", "error"]).args(args).arg(file).output();
    out.map(|o| String::from_utf8_lossy(&o.stdout).replace('\r', "")).unwrap_or_default()
}

/// One stream's packets: (pts seconds, bytes, keyframe).
fn packets(file: &Path, stream: &str) -> Vec<(f64, u64, bool)> {
    let text = ffprobe(&["-select_streams", stream, "-show_entries", "packet=pts_time,size,flags", "-of", "csv=p=0"], file);
    text.lines()
        .filter_map(|l| {
            let mut f = l.split(',');
            let t = f.next()?.parse().ok()?;
            let size = f.next()?.parse().ok()?;
            Some((t, size, f.next().is_some_and(|k| k.starts_with('K'))))
        })
        .collect()
}

/// Kilobits a second over the packets after the first `skip` seconds, and
/// the busiest whole second among them.
fn rates(list: &[(f64, u64, bool)], skip: f64) -> (f64, f64) {
    let Some(first) = list.first().map(|p| p.0 + skip) else { return (0.0, 0.0) };
    let kept: Vec<_> = list.iter().filter(|p| p.0 >= first).collect();
    let (Some(a), Some(b)) = (kept.first(), kept.last()) else { return (0.0, 0.0) };
    let bits: u64 = kept.iter().map(|p| p.1 * 8).sum();
    let mut seconds = std::collections::BTreeMap::<i64, u64>::new();
    for p in &kept {
        *seconds.entry((p.0 - a.0).floor() as i64).or_default() += p.1 * 8;
    }
    let whole = seconds.len().saturating_sub(1).max(1);
    let peak = seconds.values().take(whole).max().copied().unwrap_or(0);
    (bits as f64 / (b.0 - a.0).max(0.001) / 1000.0, peak as f64 / 1000.0)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_custom_720p_destination_on_a_programme_channel_sends_720p_at_the_asked_bitrate() {
    if !godwinmix_core::probe::have_or_skip("rtmp2sink") {
        return;
    }
    let (dir, port) = folder_with_relay("feed-custom");
    let file = dir.join("dest.flv");
    let rtmp = free_port();
    let Some(_server) = destination(rtmp, &file) else {
        return eprintln!("ffmpeg is not installed; the custom format test needs it as an RTMP server");
    };
    let plugin = staged_ingest(&dir);
    let (_st, mut ws) = ready(&dir, port).await;
    let asked = call(&mut ws, 1, "plugin.add", json!({"source": plugin.to_string_lossy()})).await;
    assert!(asked.get("error").is_none(), "{asked}");
    // A moving picture, so the encoder has something to spend its bits on.
    let ball = call(&mut ws, 2, "source.add", json!({"id": "ball", "uri": "test://ball"})).await;
    assert!(ball.get("error").is_none(), "{ball}");
    call(&mut ws, 3, "program.take", json!({"source": "ball"})).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    let added = call(&mut ws, 4, "channel.add", json!({"name": "Sunday", "feed": {"kind": "programme"}})).await;
    assert!(added.get("error").is_none(), "{added}");
    let dest = call(&mut ws, 5, "channel.destination.add", json!({"id": "sunday", "platform": "custom", "label": "Custom RTMP",
        "server": format!("rtmp://127.0.0.1:{rtmp}/live"), "key": "dest", "rendition": custom_720p()})).await;
    assert!(dest.get("error").is_none(), "{dest}");

    let live = until(&mut ws, "the destination going live", Duration::from_secs(120), |c| c["destinations"][0]["state"] == "live").await;
    let plan = &live["destinations"][0]["plan"];
    assert_eq!(plan["mode"], "transcode", "a 1080p feed is encoded for a 720p ask: {plan}");
    assert_eq!((plan["video"]["width"].as_u64(), plan["video"]["height"].as_u64()), (Some(1280), Some(720)), "{plan}");
    eprintln!("coming in: {}", live["streams"][0]);
    eprintln!("plan: {plan}");

    // Twenty seconds of what went out, with the destination's own readings.
    let mut readings = Vec::new();
    let since = Instant::now();
    let mut id = 100;
    while since.elapsed() < Duration::from_secs(20) {
        id += 1;
        readings.push(channel(&mut ws, id, "sunday").await["destinations"][0]["kbps"].as_u64().unwrap_or(0));
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    eprintln!("the destination's kbps, a second apart: {readings:?}");
    drop(_server);
    tokio::time::sleep(Duration::from_millis(500)).await;

    let shape = ffprobe(&["-select_streams", "v:0", "-show_entries", "stream=codec_name,profile,width,height,r_frame_rate", "-of", "csv=p=0"], &file);
    let sound = ffprobe(&["-select_streams", "a:0", "-show_entries", "stream=codec_name,sample_rate,channels", "-of", "csv=p=0"], &file);
    eprintln!("received video: {}, audio: {}", shape.trim(), sound.trim());
    assert!(shape.starts_with("h264,"), "{shape}");
    assert!(shape.contains(",1280,720,"), "the far end gets the 720p asked for: {shape}");
    assert!(shape.contains("30/1"), "at 30 fps: {shape}");
    assert!(sound.starts_with("aac,48000,2"), "{sound}");

    let video = packets(&file, "v:0");
    let audio = packets(&file, "a:0");
    let keys: Vec<f64> = video.iter().filter(|p| p.2).map(|p| p.0).collect();
    let gaps: Vec<f64> = keys.windows(2).map(|w| w[1] - w[0]).collect();
    let (v, v_peak) = rates(&video, 4.0);
    let (a, _) = rates(&audio, 4.0);
    eprintln!("received: video {v:.0} kb/s (busiest second {v_peak:.0}), audio {a:.0} kb/s, keyframes {gaps:?} s apart");
    assert!(gaps.iter().skip(1).all(|g| (*g - 2.0).abs() < 0.2), "a keyframe every 2 s: {gaps:?}");
    assert!(v <= 3000.0 * 1.05, "never more than the 3000 kb/s asked, beyond a measured wobble: {v:.0}");
    assert!(v >= 3000.0 * 0.5, "and not starved either: {v:.0}");
    assert!(a <= 128.0 * 1.1, "AAC at the 128 kb/s asked: {a:.0}");
}
