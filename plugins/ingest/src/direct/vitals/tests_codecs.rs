//! The silence check against MPEG-TS feeds over UDP, through the direct
//! host the way a headend feed arrives: AAC and MPEG layer II, each with a
//! tone and each really silent. Each takes a free port of its own, and each
//! skips, saying so, without ffmpeg.

use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use crate::direct::{Emit, Host, Relay};
use crate::hub::Hub;

type Heard = Arc<Mutex<Vec<(String, Value)>>>;

const TONE: &str = "sine=frequency=660:sample_rate=48000";
const QUIET: &str = "anullsrc=r=48000:cl=stereo";

/// An ffmpeg sending, killed when dropped.
struct Sender(Child);

impl Drop for Sender {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// An H.264 picture and `audio` (an ffmpeg lavfi source) coded as `codec`,
/// sent live as MPEG-TS to `port` on the loopback: the scale harness's clips,
/// made by the same tool.
fn feed(port: u16, codec: &str, audio: &str) -> Option<Sender> {
    let url = format!("udp://127.0.0.1:{port}?pkt_size=1316");
    let args = ["-loglevel", "error", "-re", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25", "-f", "lavfi", "-i", audio,
        "-t", "20", "-c:v", "libx264", "-preset", "ultrafast", "-g", "25", "-c:a", codec, "-b:a", "192k", "-f", "mpegts", &url];
    match Command::new("ffmpeg").args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
        Ok(child) => Some(Sender(child)),
        Err(e) => {
            eprintln!("skipping: needs ffmpeg on PATH ({e})");
            None
        }
    }
}

/// A UDP port nothing is bound to just now.
fn free_port() -> u16 {
    std::net::UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// A direct show reading `port`, its silence check on at two seconds and the
/// picture checks off, and every `direct.health` it says.
fn show(port: u16) -> (Arc<Host>, Heard) {
    let heard: Heard = Arc::default();
    let h = heard.clone();
    let emit: Emit = Arc::new(move |name, params| h.lock().unwrap().push((name.to_string(), params)));
    let relay: Relay = Arc::new(|| "127.0.0.1:1935".to_string());
    let host = Host::new(Hub::new(), emit, relay);
    let thresholds = json!({"black_secs": 0, "freeze_secs": 0, "silence_secs": 2, "stall_secs": 3});
    let row = json!({"id": "feed", "name": "feed", "input": {"uri": format!("udp://127.0.0.1:{port}")}, "outputs": [],
        "monitor": {"alarms": true, "pictures": false, "thresholds": thresholds}});
    host.apply(&json!({"direct": [row]}));
    (host, heard)
}

fn kinds(v: &Value) -> Vec<String> {
    let alarms = v["health"]["alarms"].as_array().cloned().unwrap_or_default();
    alarms.iter().map(|a| a["kind"].as_str().unwrap_or("").to_string()).collect()
}

/// The health events heard over `secs`, or `None` without ffmpeg.
fn run(codec: &str, audio: &str, secs: u64) -> Option<Vec<Value>> {
    let port = free_port();
    let (host, heard) = show(port);
    let _feed = feed(port, codec, audio)?;
    for _ in 0..secs {
        std::thread::sleep(Duration::from_secs(1));
        eprintln!("DIAG {}", host.stats(None)["shows"][0]["input"]);
    }
    drop(host);
    let h = heard.lock().unwrap();
    Some(h.iter().filter(|(n, _)| n == "direct.health").map(|(_, v)| v.clone()).collect())
}

fn ever(events: &[Value], kind: &str) -> bool {
    events.iter().any(|e| kinds(e).iter().any(|k| k == kind))
}

fn last_is_ok(events: &[Value]) -> bool {
    events.last().is_some_and(|e| kinds(e).is_empty())
}

#[test]
fn an_mp2_tone_is_not_silence() {
    let Some(events) = run("mp2", TONE, 12) else { return };
    assert!(!ever(&events, "silence"), "{events:#?}");
    assert!(last_is_ok(&events), "{events:#?}");
}

#[test]
fn an_aac_tone_is_not_silence() {
    let Some(events) = run("aac", TONE, 12) else { return };
    assert!(!ever(&events, "silence"), "{events:#?}");
    assert!(last_is_ok(&events), "{events:#?}");
}

#[test]
fn silent_mp2_is_silence() {
    let Some(events) = run("mp2", QUIET, 12) else { return };
    assert!(ever(&events, "silence"), "{events:#?}");
}

#[test]
fn silent_aac_is_silence() {
    let Some(events) = run("aac", QUIET, 12) else { return };
    assert!(ever(&events, "silence"), "{events:#?}");
}

#[test]
fn a_feed_that_turns_from_aac_to_mp2_is_not_silence() {
    let port = free_port();
    let (host, heard) = show(port);
    let Some(aac) = feed(port, "aac", TONE) else { return };
    std::thread::sleep(Duration::from_secs(5));
    drop(aac);
    std::thread::sleep(Duration::from_secs(4));
    heard.lock().unwrap().clear();
    let Some(_mp2) = feed(port, "mp2", TONE) else { return };
    for _ in 0..12 {
        std::thread::sleep(Duration::from_secs(1));
        eprintln!("DIAG {}", host.stats(None)["shows"][0]["input"]);
    }
    let h = heard.lock().unwrap();
    let events: Vec<Value> = h.iter().filter(|(n, _)| n == "direct.health").map(|(_, v)| v.clone()).collect();
    assert!(!ever(&events, "silence"), "{events:#?}");
}

#[test]
fn probe_aac_caps_on_mp2_frames() {
    use gstreamer as gst;
    use gstreamer::prelude::*;
    gmx_netkit::init().unwrap();
    for wave in ["sine", "silence"] {
        let line = format!("audiotestsrc num-buffers=20 wave={wave} ! audio/x-raw,rate=48000,channels=2 ! avenc_mp2 ! mpegaudioparse ! appsink name=s sync=false");
        let p = gst::parse::launch(&line).unwrap().downcast::<gst::Pipeline>().unwrap();
        let sink = p.by_name("s").unwrap().downcast::<gstreamer_app::AppSink>().unwrap();
        p.set_state(gst::State::Playing).unwrap();
        let mut tags = Vec::new();
        let mut ms = 0u32;
        while let Ok(s) = sink.pull_sample() {
            let map = s.buffer().unwrap().map_readable().unwrap();
            let mut body = crate::exaudio::prefix(crate::exaudio::MPEG).to_vec();
            body.extend_from_slice(map.as_slice());
            tags.push(crate::media_tag::MediaTag { kind: crate::media_tag::TagKind::Audio, timestamp_ms: ms, keyframe: false, sequence_header: false, payload: std::sync::Arc::from(body) });
            ms += 24;
        }
        let _ = p.set_state(gst::State::Null);
        let header = crate::media_tag::MediaTag { kind: crate::media_tag::TagKind::Audio, timestamp_ms: 0, keyframe: false, sequence_header: true, payload: std::sync::Arc::from(vec![0xAF, 0x00, 0x11, 0x90]) };
        let caps = crate::transcode::input::caps_for(&header).unwrap();
        let mut chain = super::measure::chain_for("audio/mpeg", false).unwrap();
        for burst in tags.chunks(3).take(5) {
            let bufs = burst.iter().filter_map(|t| crate::transcode::input::buffer(t, burst[0].timestamp_ms)).collect();
            let out = chain.run(&caps, bufs);
            let peak = out.iter().skip(1).filter_map(super::measure::peak).reduce(f64::max);
            eprintln!("PROBE {wave}: {} samples out, peak {:?}", out.len(), peak.map(super::measure::to_db));
        }
    }
}
