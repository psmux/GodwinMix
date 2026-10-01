//! What the benches read off the process, and one 1080p keyframe through
//! each way of decoding it, timed.

use std::time::{Duration, Instant};

use gstreamer as gst;

use super::clip;
use crate::media_tag::TagKind;
use crate::transcode::input::{buffer, caps_for};

/// This process's CPU seconds and resident MB, from `ps`.
fn usage() -> (f64, f64) {
    let out = std::process::Command::new("ps").args(["-o", "time=,rss=", "-p", &std::process::id().to_string()]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let mut f = text.split_whitespace();
    let time = f.next().unwrap_or("0:0");
    let secs = time.split(':').fold(0.0, |acc, part| acc * 60.0 + part.parse::<f64>().unwrap_or(0.0));
    (secs, f.next().unwrap_or("0").parse::<f64>().unwrap_or(0.0) / 1024.0)
}

/// CPU in percent of one core and resident MB over `secs`.
pub fn phase(name: &str, secs: u64) -> (f64, f64) {
    let (c0, _) = usage();
    let t0 = Instant::now();
    std::thread::sleep(Duration::from_secs(secs));
    let (c1, mb) = usage();
    let cpu = 100.0 * (c1 - c0) / t0.elapsed().as_secs_f64();
    eprintln!("BENCH {name}: {cpu:.1}% of one core, {mb:.0} MB resident");
    (cpu, mb)
}

/// Milliseconds of wall time per keyframe for each way of decoding one.
#[test]
#[ignore]
fn one_keyframe_each_way() {
    let clip = clip();
    let header = clip.iter().find(|t| t.sequence_header && t.kind == TagKind::Video).unwrap();
    let key = clip.iter().find(|t| t.keyframe && !t.sequence_header).unwrap();
    let caps = caps_for(header).unwrap();
    eprintln!("BENCH keyframe of {} KB", key.payload.len() / 1024);
    let out = || gst::Caps::builder("video/x-raw").field("format", "I420").field("width", 320i32).build();
    let ways: [&[&str]; 5] = [
        &["h264parse", "vtdec_hw", "videoscale", "videoconvert"],
        &["avdec_h264", "videoscale", "videoconvert"],
        &["avdec_h264", "identity"],
        &["h264parse", "vtdec_hw", "identity"],
        &["avdec_h264", "videoconvertscale"],
    ];
    for way in ways {
        let Ok(mut chain) = super::super::chain::Chain::new(way, if way.contains(&"identity") { gst::Caps::new_any() } else { out() }) else { continue };
        let t0 = Instant::now();
        for _ in 0..50 {
            if chain.run(&caps, vec![buffer(key, key.timestamp_ms).unwrap()]).is_empty() { eprintln!("BENCH   empty"); }
        }
        eprintln!("BENCH {way:?}: {:.1} ms a keyframe", t0.elapsed().as_secs_f64() * 1000.0 / 50.0);
    }
}
