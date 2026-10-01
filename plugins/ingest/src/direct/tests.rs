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
use crate::media_tag::{MediaTag, TagKind};
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
    assert!(pictures >= 120, "the live output kept its frame rate: {pictures} pictures in 5 s");
    assert!(blocked.dropped_gops() > 0 || blocked.waiting().0 > 0, "the blocked reader held or lost GOPs of its own");
    let worst = slowest.load(Ordering::Relaxed);
    assert!(worst < 20_000, "the input never waited on an output: slowest push {worst} µs");
    let states = host.stats(None)["shows"][0]["outputs"].clone();
    assert_ne!(states[0]["state"], "live", "{states}");
    drop(mute);
}

#[test]
fn a_changed_table_touches_only_what_changed() {
    let hub = Hub::new();
    let (encoder, _) = channel(&hub);
    let (host, _) = host(&hub);
    let a = UdpSocket::bind("127.0.0.1:0").unwrap();
    let url_a = format!("udp://127.0.0.1:{}", a.local_addr().unwrap().port());
    let b = UdpSocket::bind("127.0.0.1:0").unwrap();
    let url_b = format!("udp://127.0.0.1:{}", b.local_addr().unwrap().port());
    let one = json!({"direct": [row("x", json!([out("a", &url_a)])), row("y", json!([]))]});
    host.apply(&one);
    let reading = listen_udp(a, 5);
    std::thread::sleep(Duration::from_secs(2));
    // A second output, the other show gone, the first output untouched.
    host.apply(&json!({"direct": [row("x", json!([out("a", &url_a), out("b", &url_b)]))]}));
    assert!(!hub.is_live("direct.y", "main"), "a row gone stops its show at once");
    let got_b = listen_udp(b, 2).join().unwrap();
    let got_a = reading.join().unwrap();
    let _ = encoder.set_state(gst::State::Null);
    assert!(decode(&got_b, "b").0 >= 30, "the new output sends");
    // The first output's muxer never started again: one continuity run on
    // the video PID from start to end, so one output thread throughout.
    let cc: Vec<u8> = got_a
        .chunks(188)
        .filter(|p| ((u16::from(p[1] & 0x1f) << 8) | u16::from(p[2])) == 0x100)
        .map(|p| p[3] & 0x0f)
        .collect();
    let breaks = cc.windows(2).filter(|w| w[1] != (w[0] + 1) & 0x0f).count();
    assert!(cc.len() > 500 && breaks == 0, "{} video packets, {breaks} breaks", cc.len());
    assert_eq!(host.rows().len(), 1);
}

#[test]
fn an_input_that_cannot_open_says_why_and_its_show_still_runs() {
    let hub = Hub::new();
    let (host, heard) = host(&hub);
    host.apply(&json!({"direct": [{"id": "bad", "input": {"uri": "carrier-pigeon://x"}, "outputs": []}]}));
    wait_for("direct.input", 5, || !said(&heard, "direct.input").is_empty());
    let input = &said(&heard, "direct.input")[0];
    assert_eq!(input["state"], "idle");
    assert!(input["error"].as_str().unwrap_or("").contains("carrier-pigeon"), "{input}");
}

#[test]
fn a_rendition_is_decoded_once_and_sent_at_the_size_asked_for() {
    let hub = Hub::new();
    let (encoder, _) = channel(&hub);
    let (host, _) = host(&hub);
    let enc = "encode:main:h264:160x90p30:150k:g1000";
    let nodes = json!([
        {"id": "decode:main:video", "kind": "decode", "track": "video", "codec": "h264", "element": "avdec_h264", "parser": "h264parse"},
        {"id": "scale:main:160x90p30", "kind": "scale", "input": "decode:main:video", "width": 160, "height": 90, "fps": [30, 1]},
        {"id": enc, "kind": "encode", "input": "scale:main:160x90p30", "codec": "h264", "element": "x264enc", "parser": "h264parse",
         "width": 160, "height": 90, "fps": [30, 1], "bitrate_kbps": 150,
         "props": {"tune": "zerolatency", "speed-preset": "ultrafast", "bitrate": 150, "key-int-max": 30, "bframes": 0, "byte-stream": false}}
    ]);
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let url = format!("udp://127.0.0.1:{}", socket.local_addr().unwrap().port());
    let mut r = row("small", json!([{"id": "small", "platform": "custom", "url": url, "stream": "main",
                                      "rendition": true, "video": enc, "audio": "copy:main:audio"}]));
    r["transcode"] = json!([{"stream": "main", "nodes": nodes}]);
    host.apply(&json!({"direct": [r]}));
    let renditions = host.transcoders.renditions();
    let key = crate::transcode::output_key("main", Some(enc), Some("copy:main:audio"));
    wait_for("the converted pair", 20, || renditions.is_live("direct.small", &key));
    // The decode the rendition already runs hands a picture a second to
    // whoever asks, at the input's own size.
    let tapped = Arc::new(Mutex::new(Vec::new()));
    let p = tapped.clone();
    let tap: crate::transcode::Tap = Arc::new(move |s: &gst::Sample| {
        let caps = s.caps().and_then(|c| c.structure(0).map(|st| (st.name().to_string(), st.get::<i32>("width").unwrap_or(0))));
        p.lock().unwrap().push(caps);
    });
    host.transcoders.set_tap("direct.small", "main", Some(tap));
    let listening = listen_udp(socket, 4);
    // A late reader is given the encoder's own sequence header first.
    let late = renditions.subscribe("direct.small", &key);
    let header = (0..100).find_map(|_| match late.recv_timeout(Duration::from_millis(50)) {
        crate::hub::Recv::Tag(t) if t.kind == TagKind::Video && t.sequence_header => Some(t),
        _ => None,
    });
    let got = listening.join().unwrap();
    let _ = encoder.set_state(gst::State::Null);
    assert_eq!(header.map(|t| crate::codec::read_video(&t).width), Some(160), "the size asked for");
    let (pictures, _) = decode(&got, "small");
    assert!(pictures >= 60, "decoded {pictures} pictures of the rendition");
    let stats = host.stats(None);
    assert_eq!(stats["shows"][0]["outputs"][0]["encoder"], "x264enc", "{stats}");
    let seen = tapped.lock().unwrap().clone();
    assert!((2..=6).contains(&seen.len()), "about one picture a second, not every frame: {seen:?}");
    assert!(seen.iter().all(|c| c.as_ref().is_some_and(|(name, w)| name == "video/x-raw" && *w == 320)), "{seen:?}");
}

#[test]
fn rtp_carries_the_same_stream_with_a_sequence_number_on_each_datagram() {
    let hub = Hub::new();
    let (encoder, _) = channel(&hub);
    let (host, _) = host(&hub);
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let url = format!("rtp://127.0.0.1:{}", socket.local_addr().unwrap().port());
    host.apply(&json!({"direct": [row("rtp", json!([{"id": "rtp", "platform": "rtp", "url": url, "stream": "main"}]))]}));
    socket.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
    let (mut ts, mut seqs, mut buf) = (Vec::new(), Vec::new(), [0u8; 2048]);
    let until = Instant::now() + Duration::from_secs(4);
    while Instant::now() < until {
        if let Ok(n) = socket.recv(&mut buf) {
            assert_eq!((buf[0], buf[1]), (0x80, 33), "RTP version 2, MPEG-TS");
            seqs.push(u16::from_be_bytes([buf[2], buf[3]]));
            ts.extend_from_slice(&buf[12..n]);
        }
    }
    let _ = encoder.set_state(gst::State::Null);
    assert!(seqs.windows(2).all(|w| w[1] == w[0].wrapping_add(1)), "no sequence number skipped");
    assert!(decode(&ts, "rtp").0 >= 80, "the payload is the stream");
}

#[test]
fn rist_reaches_a_rist_receiver() {
    gmx_netkit::init().unwrap();
    if gst::ElementFactory::find("ristsrc").is_none() {
        eprintln!("skipping: needs ristsrc");
        return;
    }
    let hub = Hub::new();
    let (encoder, _) = channel(&hub);
    let (host, _) = host(&hub);
    // An even port, as RIST wants, with RTCP on the one above.
    let port = UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port() & !1;
    let path = std::env::temp_dir().join(format!("gmx-direct-rist-{}.ts", std::process::id()));
    let line = format!("ristsrc address=127.0.0.1 port={port} ! rtpmp2tdepay ! filesink location={}", path.display());
    let receiver = gst::parse::launch(&line).unwrap();
    receiver.set_state(gst::State::Playing).unwrap();
    let url = format!("rist://127.0.0.1:{port}");
    host.apply(&json!({"direct": [row("rist", json!([{"id": "rist", "platform": "rist", "url": url, "stream": "main"}]))]}));
    std::thread::sleep(Duration::from_secs(4));
    host.apply(&json!({"direct": []}));
    let _ = encoder.set_state(gst::State::Null);
    let _ = receiver.set_state(gst::State::Null);
    let (pictures, _) = testfeed::decode_ts(&path);
    let _ = std::fs::remove_file(&path);
    assert!(pictures >= 60, "decoded {pictures} pictures that came over RIST");
}
