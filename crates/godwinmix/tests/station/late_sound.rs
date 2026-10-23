//! A feed timed as a GPU encoder's reaches the hub: a keyframe every 50
//! frames at 29.97 fps (1668 ms), and the sound 0.7 s behind the picture of
//! the same moment. Encoded once with x264 and AAC into FLV tags, then
//! served in real time by a stand in for the ingest plugin's relay
//! (`GMXHUB <path> tracks=all`), so the HLS packager reads it as it reads a
//! converted channel.

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSink;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener};
use std::time::{Duration, Instant};

/// One FLV tag: when it is sent (ms from the start), its type and time, and body.
struct Tag {
    send_ms: u64,
    kind: u8,
    ms: u32,
    body: Vec<u8>,
}

fn samples(sink: &AppSink) -> Vec<gst::Sample> {
    std::iter::from_fn(|| sink.pull_sample().ok()).collect()
}

fn codec_data(s: &gst::Sample) -> Vec<u8> {
    let caps = s.caps().unwrap();
    let b = caps.structure(0).unwrap().get::<gst::Buffer>("codec_data").unwrap();
    let map = b.map_readable().unwrap();
    map.to_vec()
}

/// The tags of `seconds` of feed, in the order they are sent.
fn tags(seconds: u32) -> Vec<Tag> {
    gst::init().unwrap();
    let frames = seconds * 30;
    let text = format!(
        "videotestsrc num-buffers={frames} ! video/x-raw,width=320,height=180,framerate=30000/1001 ! x264enc key-int-max=50 bframes=0 speed-preset=ultrafast \
         ! h264parse ! video/x-h264,stream-format=avc,alignment=au ! appsink name=v sync=false \
         audiotestsrc num-buffers={} samplesperbuffer=1024 ! audio/x-raw,rate=48000,channels=2 ! avenc_aac ! aacparse ! audio/mpeg,stream-format=raw ! appsink name=a sync=false",
        seconds * 47
    );
    let p = gst::parse::launch(&text).unwrap().downcast::<gst::Pipeline>().unwrap();
    let sink = |n: &str| p.by_name(n).unwrap().downcast::<AppSink>().unwrap();
    let (v, a) = (sink("v"), sink("a"));
    p.set_state(gst::State::Playing).unwrap();
    let (vs, aus) = (samples(&v), samples(&a));
    p.set_state(gst::State::Null).unwrap();
    let mut out = vec![
        Tag { send_ms: 0, kind: 9, ms: 0, body: [&[0x17, 0, 0, 0, 0][..], &codec_data(&vs[0])].concat() },
        Tag { send_ms: 0, kind: 8, ms: 0, body: [&[0xaf, 0][..], &codec_data(&aus[0])].concat() },
    ];
    // x264enc starts its times a thousand hours in, so negative decode
    // times never happen; each stream starts at zero here.
    let zero = vs[0].buffer().unwrap().dts_or_pts().unwrap().mseconds();
    for s in &vs {
        let b = s.buffer().unwrap();
        let ms = (b.dts_or_pts().unwrap().mseconds() - zero) as u32;
        let key = !b.flags().contains(gst::BufferFlags::DELTA_UNIT);
        let body = [&[if key { 0x17 } else { 0x27 }, 1, 0, 0, 0][..], &b.map_readable().unwrap()].concat();
        out.push(Tag { send_ms: u64::from(ms), kind: 9, ms, body });
    }
    for s in &aus {
        let b = s.buffer().unwrap();
        let ms = b.pts().unwrap().mseconds() as u32;
        // The sound of a moment arrives with the picture 0.7 s later.
        out.push(Tag { send_ms: u64::from(ms) + 700, kind: 8, ms, body: [&[0xaf, 1][..], &b.map_readable().unwrap()].concat() });
    }
    out.sort_by_key(|t| t.send_ms);
    out
}

fn write(out: &mut impl Write, t: &Tag) -> std::io::Result<()> {
    let (n, ms) = (t.body.len() as u32, t.ms);
    let s = n.to_be_bytes();
    let head = [t.kind, s[1], s[2], s[3], (ms >> 16) as u8, (ms >> 8) as u8, ms as u8, (ms >> 24) as u8, 0, 0, 0];
    out.write_all(&head)?;
    out.write_all(&t.body)?;
    out.write_all(&(11 + n).to_be_bytes())
}

/// Serve `seconds` of the feed to one reader, in real time, on a thread.
/// Answers where the relay listens.
pub fn relay(seconds: u32) -> SocketAddr {
    let all = tags(seconds);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let Ok((sock, _)) = listener.accept() else { return };
        let mut line = String::new();
        let _ = BufReader::new(sock.try_clone().unwrap()).read_line(&mut line);
        let mut out = std::io::BufWriter::new(sock);
        let _ = out.write_all(b"FLV\x01\x05\x00\x00\x00\x09\x00\x00\x00\x00");
        let start = Instant::now();
        for t in &all {
            let due = start + Duration::from_millis(t.send_ms);
            if let Some(wait) = due.checked_duration_since(Instant::now()) {
                let _ = out.flush();
                std::thread::sleep(wait);
            }
            if write(&mut out, t).is_err() {
                return;
            }
        }
        let _ = out.flush();
    });
    addr
}
