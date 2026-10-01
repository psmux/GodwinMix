//! The silence check against MPEG-TS feeds over UDP, through the direct
//! host the way a headend feed arrives: AAC, MPEG layer II and AC-3, each
//! with a tone and each really silent. Each takes a free port of its own,
//! and each skips, saying so, without ffmpeg.
//!
//! A tone test alone would pass with the sound never measured, which is how
//! layer II and AC-3 went unwatched: the silent test beside it is what holds
//! the measurement to happening at all.

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
    std::thread::sleep(Duration::from_secs(secs));
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
fn an_ac3_tone_is_not_silence_and_silent_ac3_is() {
    let Some(events) = run("ac3", TONE, 12) else { return };
    assert!(!ever(&events, "silence") && last_is_ok(&events), "{events:#?}");
    let Some(events) = run("ac3", QUIET, 12) else { return };
    assert!(ever(&events, "silence"), "{events:#?}");
}
