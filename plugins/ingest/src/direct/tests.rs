//! The direct host with real sockets and real GStreamer: x264 and AAC on the
//! hub as a channel stream, shows reading it, outputs sending UDP, RTMP and
//! files, and GStreamer's own demuxer and libav reading what they sent.

use std::net::UdpSocket;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::{json, Value};

use super::{Emit, Host, Relay};
use crate::hub::{Hub, Publication};
use crate::media_tag::MediaTag;
use crate::rtmp::Inlet;
use crate::testfeed;

type Heard = Arc<Mutex<Vec<(String, Value)>>>;

fn host(hub: &Hub) -> (Arc<Host>, Heard) {
    let heard: Heard = Arc::default();
    let h = heard.clone();
    let emit: Emit = Arc::new(move |name, params| h.lock().unwrap().push((name.to_string(), params)));
    let relay: Relay = Arc::new(|| "127.0.0.1:1935".to_string());
    (Host::new(hub.clone(), emit, relay), heard)
}

/// Every push the publisher made, and the slowest.
struct Timed(Publication, Arc<AtomicU64>);

impl Inlet for Timed {
    fn tag(&mut self, tag: MediaTag) {
        let t0 = Instant::now();
        self.0.push(tag);
        self.1.fetch_max(t0.elapsed().as_micros() as u64, Ordering::Relaxed);
    }
}

/// A live encoder publishing `church/main` on `hub`, as a channel stream.
fn channel(hub: &Hub) -> (gst::Pipeline, Arc<AtomicU64>) {
    let p = hub.publish("church", "main", "127.0.0.1:1", None).unwrap();
    let slowest = Arc::new(AtomicU64::new(0));
    (testfeed::live(Box::new(Timed(p, slowest.clone()))), slowest)
}

fn row(id: &str, outputs: Value) -> Value {
    json!({"id": id, "name": id, "input": {"uri": "channel:church/main"}, "outputs": outputs, "monitor": {"alarms": false, "pictures": false}})
}

fn out(id: &str, url: &str) -> Value {
    json!({"id": id, "platform": "custom", "url": url, "stream": "main"})
}

/// Everything that arrives on a UDP port for `secs`.
fn listen_udp(socket: UdpSocket, secs: u64) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        socket.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
        let (mut got, until, mut buf) = (Vec::new(), Instant::now() + Duration::from_secs(secs), [0u8; 2048]);
        while Instant::now() < until {
            if let Ok(n) = socket.recv(&mut buf) {
                got.extend_from_slice(&buf[..n]);
            }
        }
        got
    })
}

fn decode(ts: &[u8], name: &str) -> (u32, u32) {
    let path = std::env::temp_dir().join(format!("gmx-direct-{name}-{}.ts", std::process::id()));
    std::fs::write(&path, ts).unwrap();
    let counts = testfeed::decode_ts(&path);
    let _ = std::fs::remove_file(&path);
    counts
}

fn wait_for(what: &str, secs: u64, mut ok: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(secs);
    while !ok() {
        assert!(Instant::now() < until, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn said(heard: &Heard, name: &str) -> Vec<Value> {
    heard.lock().unwrap().iter().filter(|(n, _)| n == name).map(|(_, v)| v.clone()).collect()
}

#[test]
fn a_copy_over_udp_is_the_input_s_own_frames_and_decodes() {
    let hub = Hub::new();
    let (encoder, _) = channel(&hub);
    let (host, heard) = host(&hub);
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let url = format!("udp://127.0.0.1:{}", socket.local_addr().unwrap().port());
    host.apply(&json!({"direct": [row("bbc-one", json!([out("udp", &url)]))]}));
    let got = listen_udp(socket, 4).join().unwrap();
    let _ = encoder.set_state(gst::State::Null);
    assert_eq!(got.len() % 188, 0, "whole transport packets");
    let (pictures, sound) = decode(&got, "udp");
    assert!(pictures >= 80, "decoded {pictures} pictures from about 4 s at 30 fps");
    assert!(sound >= 80, "and {sound} sound frames");
    let inputs = said(&heard, "direct.input");
    let live = inputs.iter().find(|v| v["state"] == "live").expect("direct.input said live");
    assert_eq!(live["stream"], "direct.bbc-one/main");
    assert_eq!(live["video"]["codec"], "h264");
    assert!(said(&heard, "direct.output").iter().any(|v| v["output"] == "udp" && v["state"] == "live"));
    let stats = host.stats(None);
    assert_eq!(stats["shows"][0]["input"]["width"], 320, "{stats}");
    assert!(stats["shows"][0]["outputs"][0]["kbps"].as_u64().unwrap() > 100, "{stats}");
}

#[test]
fn a_recording_is_a_file_every_player_opens() {
    let hub = Hub::new();
    let (encoder, _) = channel(&hub);
    let (host, _) = host(&hub);
    let dir = std::env::temp_dir().join(format!("gmx-direct-rec-{}", std::process::id()));
    let path = dir.join("show.ts");
    host.apply(&json!({"direct": [row("rec", json!([out("rec", &format!("file://{}", path.display()))]))]}));
    std::thread::sleep(Duration::from_secs(3));
    host.apply(&json!({"direct": []}));
    let _ = encoder.set_state(gst::State::Null);
    std::thread::sleep(Duration::from_millis(500));
    let (pictures, _) = testfeed::decode_ts(&path);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(pictures >= 50, "decoded {pictures} pictures from a 3 s recording");
}

#[test]
fn a_dead_output_slows_neither_the_input_nor_the_live_output() {
    let hub = Hub::new();
    let (encoder, slowest) = channel(&hub);
    let (host, _) = host(&hub);
    // A server that takes the connection and never says a word back.
    let mute = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = format!("rtmp://127.0.0.1:{}/live/x", mute.local_addr().unwrap().port());
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let url = format!("udp://127.0.0.1:{}", socket.local_addr().unwrap().port());
    host.apply(&json!({"direct": [row("s", json!([out("dead", &dead), out("udp", &url)]))]}));
    // And a reader of the show that never reads at all.
    let blocked = hub.subscribe("direct.s", "main");
    let got = listen_udp(socket, 5).join().unwrap();
    let _ = encoder.set_state(gst::State::Null);
    let (pictures, _) = decode(&got, "dead");
    // 120 of 150 alone; 100 on a runner that declares itself slow, where a
    // macOS runner counted 119. An output held up by the dead one would have
    // a handful.
    let slow = std::env::var("GODWINMIX_TIMING_SLACK").ok().and_then(|s| s.parse::<f64>().ok()).is_some_and(|s| s > 1.0);
    let floor = if slow { 100 } else { 120 };
    assert!(pictures >= floor, "the live output kept its frame rate: {pictures} pictures in 5 s");
    assert!(blocked.dropped_gops() > 0 || blocked.waiting().0 > 0, "the blocked reader held or lost GOPs of its own");
    let worst = slowest.load(Ordering::Relaxed);
    assert!(worst < 20_000, "the input never waited on an output: slowest push {worst} µs");
    let states = host.stats(None)["shows"][0]["outputs"].clone();
    assert_ne!(states[0]["state"], "live", "{states}");
    drop(mute);
}

#[path = "tests_plan.rs"]
mod plan;

#[path = "tests_carriage.rs"]
mod carriage;
