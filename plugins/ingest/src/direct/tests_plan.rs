//! A row whose input cannot be read, the calls the station makes, and a
//! rendition built from the station's plan.

use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::json;

use super::{channel, decode, host, listen_udp, out, row, said, wait_for};
use crate::hub::Hub;
use crate::media_tag::TagKind;

#[test]
fn an_input_that_cannot_open_says_why_and_its_show_still_runs() {
    let hub = Hub::new();
    let (host, heard) = host(&hub);
    host.apply(&json!({"direct": [{"id": "bad", "input": {"uri": "carrier-pigeon://x"}, "outputs": []}]}));
    wait_for("direct.input", 5, || !said(&heard, "direct.input").is_empty());
    let input = &said(&heard, "direct.input")[0];
    assert_eq!(input["state"], "idle");
    assert!(input["error"].as_str().unwrap_or("").contains("carrier-pigeon"), "{input}");
    // What the station calls answers for it, and for nothing else.
    let stats = host.call("direct.stats", &json!({"ids": ["bad"]})).unwrap();
    assert_eq!(stats["shows"][0]["id"], "bad");
    assert_eq!(host.call("direct.stats", &json!({"ids": ["other"]})).unwrap()["shows"], json!([]));
    assert!(host.call("direct.thumbnail", &json!({"show": "bad"})).is_some());
    assert!(host.call("direct.teleport", &json!({})).is_none());
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
    // Two seconds, times GODWINMIX_TIMING_SLACK on a runner that says it is
    // slow: on a macOS runner the new output's first two seconds held fewer
    // than 30 pictures with the suite beside it.
    let slack = std::env::var("GODWINMIX_TIMING_SLACK").ok().and_then(|s| s.parse::<f64>().ok()).unwrap_or(1.0).max(1.0);
    let got_b = listen_udp(b, (2.0 * slack).ceil() as u64).join().unwrap();
    let got_a = reading.join().unwrap();
    let _ = encoder.set_state(gst::State::Null);
    let pictures_b = decode(&got_b, "b").0;
    assert!(pictures_b >= 30, "the new output sends: {pictures_b} pictures");
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
