//! RTP and RIST, against real receivers.

use std::net::UdpSocket;
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::json;

use super::{channel, decode, host, row};
use crate::hub::Hub;
use crate::testfeed;

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
