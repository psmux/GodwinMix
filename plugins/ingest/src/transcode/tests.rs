//! Converting a live stream with real GStreamer: a real H.264 and AAC
//! publisher on the hub, the nodes the core would hand over, and readers of
//! the pairs the way the restream senders read them.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::{json, Value};

use super::*;
use crate::codec;
use crate::hub::{Publication, Recv};
use crate::media_tag::{MediaTag, TagKind};
use crate::rtmp::Inlet;
use crate::sends::{Feed, Sends, Wanted};
use crate::tagger;

const ENCODE: &str = "encode:main:h264:320x180p30:300k:g1000";
const SMALL: &str = "encode:main:h264:160x90p30:150k:g1000";

/// Every push the publisher made, and the slowest.
struct Timed(Publication, Arc<AtomicU64>);

impl Inlet for Timed {
    fn tag(&mut self, tag: MediaTag) {
        let t0 = Instant::now();
        self.0.push(tag);
        self.1.fetch_max(t0.elapsed().as_micros() as u64, Ordering::Relaxed);
    }
}

/// A 640x360 30 fps H.264 and AAC publisher on `hub`, as an encoder would
/// send it, until the pipeline is dropped. Answers the slowest push in µs.
fn publish(hub: &Hub) -> (gst::Pipeline, Arc<AtomicU64>) {
    gmx_netkit::init().unwrap();
    let p = hub.publish("church", "main", "127.0.0.1:1", None).unwrap();
    let slowest = Arc::new(AtomicU64::new(0));
    let to = tagger::share(Box::new(Timed(p, slowest.clone())));
    let zero = Arc::new(tagger::Zero::default());
    let pipeline = gst::parse::launch(
        "videotestsrc is-live=true pattern=ball ! video/x-raw,width=640,height=360,framerate=30/1 \
         ! x264enc tune=zerolatency speed-preset=ultrafast key-int-max=30 bframes=0 bitrate=800 ! h264parse name=vp \
         audiotestsrc is-live=true ! audio/x-raw,rate=48000,channels=2 ! avenc_aac ! aacparse name=ap",
    )
    .unwrap()
    .downcast::<gst::Pipeline>()
    .unwrap();
    for (parser, sink) in [("vp", tagger::video_sink(to.clone(), zero.clone())), ("ap", tagger::audio_sink(to, zero))] {
        pipeline.add(&sink).unwrap();
        pipeline.by_name(parser).unwrap().link(&sink).unwrap();
    }
    pipeline.set_state(gst::State::Playing).unwrap();
    (pipeline, slowest)
}

fn encode(id: &str, input: &str, width: u32, height: u32, kbps: u32) -> Value {
    json!({"id": id, "kind": "encode", "input": input, "codec": "h264", "element": "x264enc", "parser": "h264parse",
           "width": width, "height": height, "fps": [30, 1], "bitrate_kbps": kbps,
           "props": {"tune": "zerolatency", "speed-preset": "ultrafast", "bitrate": kbps, "key-int-max": 30, "bframes": 0, "byte-stream": false}})
}

fn spec(small: bool) -> Vec<StreamSpec> {
    let mut nodes = vec![
        json!({"id": "decode:main:video", "kind": "decode", "track": "video", "codec": "h264", "element": "avdec_h264", "parser": "h264parse"}),
        json!({"id": "scale:main:320x180p30", "kind": "scale", "input": "decode:main:video", "width": 320, "height": 180, "fps": [30, 1]}),
        encode(ENCODE, "scale:main:320x180p30", 320, 180, 300),
    ];
    if small {
        nodes.push(json!({"id": "scale:main:160x90p30", "kind": "scale", "input": "decode:main:video", "width": 160, "height": 90, "fps": [30, 1]}));
        nodes.push(encode(SMALL, "scale:main:160x90p30", 160, 90, 150));
    }
    specs(&json!({"channels": [{"id": "church", "app": "church", "transcode": [{"stream": "main", "nodes": nodes}]}]}))
}

fn wanted(id: &str, video: &str, url: &str) -> Wanted {
    let feed = Feed::Rendition { video: Some(video.into()), audio: Some("copy:main:audio".into()) };
    Wanted { channel: "church".into(), app: "church".into(), id: id.into(), platform: "custom".into(), url: url.into(), stream: "main".into(), feed }
}

fn wait_for(what: &str, secs: u64, mut ok: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(secs);
    while !ok() {
        assert!(Instant::now() < until, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Read a pair for `secs`, answering its tags.
fn read_for(hub: &Hub, key: &str, secs: u64) -> Vec<MediaTag> {
    let reader = hub.subscribe("church", key);
    let (mut out, until) = (Vec::new(), Instant::now() + Duration::from_secs(secs));
    while Instant::now() < until {
        match reader.recv_timeout(Duration::from_millis(100)) {
            Recv::Tag(t) => out.push(t),
            Recv::Ended => break,
            Recv::Timeout => {}
        }
    }
    out
}

fn size(tags: &[MediaTag]) -> Option<(u32, u32)> {
    let header = tags.iter().find(|t| t.kind == TagKind::Video && t.sequence_header)?;
    let v = codec::read_video(header);
    Some((v.width, v.height))
}

fn first_tags(tags: &[MediaTag]) -> Vec<(TagKind, u32, bool, bool)> {
    tags.iter().take(16).map(|t| (t.kind, t.timestamp_ms, t.keyframe, t.sequence_header)).collect()
}

#[test]
fn three_destinations_of_one_pair_read_one_encoder_s_bytes_at_the_size_asked_for() {
    let hub = Hub::new();
    let (source, _) = publish(&hub);
    let t = Transcoders::new(hub.clone());
    let w: Vec<Wanted> = ["a", "b", "c"].iter().map(|id| wanted(id, ENCODE, "rtmp://x/y/z")).collect();
    t.apply(spec(false), &w);
    let key = output_key("main", Some(ENCODE), Some("copy:main:audio"));
    let renditions = t.renditions();
    wait_for("the converted pair", 15, || renditions.is_live("church", &key));
    let readers: Vec<_> = (0..3)
        .map(|_| {
            let (h, k) = (renditions.clone(), key.clone());
            std::thread::spawn(move || read_for(&h, &k, 3))
        })
        .collect();
    let got: Vec<Vec<MediaTag>> = readers.into_iter().map(|r| r.join().unwrap()).collect();
    for tags in &got {
        assert_eq!(size(tags), Some((320, 180)), "the encoder's own sequence header, at the size asked for");
        let first = tags.iter().find(|t| t.kind == TagKind::Video && !t.sequence_header).unwrap();
        assert!(first.keyframe, "every reader starts on a keyframe");
        assert!(tags.iter().filter(|t| t.kind == TagKind::Video).count() > 45, "{}", tags.len());
        let sound = tags.iter().any(|t| t.kind == TagKind::Audio && !t.sequence_header);
        assert!(sound, "the stream's own sound rides along: {:?}", first_tags(tags));
    }
    let shared = got[0].iter().filter(|a| got[1].iter().any(|b| Arc::ptr_eq(&a.payload, &b.payload))).count();
    assert!(shared > 45, "the three read one encoder's bytes, not three encoders': {shared} shared");
    drop(source);
}

#[test]
fn adding_a_size_to_a_running_stream_leaves_the_pair_already_sending_alone() {
    let hub = Hub::new();
    let (source, _) = publish(&hub);
    let t = Transcoders::new(hub.clone());
    let first = vec![wanted("a", ENCODE, "rtmp://x/y/z")];
    t.apply(spec(false), &first);
    let key = output_key("main", Some(ENCODE), Some("copy:main:audio"));
    let renditions = t.renditions();
    wait_for("the first pair", 15, || renditions.is_live("church", &key));
    let (h, k) = (renditions.clone(), key.clone());
    let reading = std::thread::spawn(move || read_for(&h, &k, 5));
    std::thread::sleep(Duration::from_secs(1));
    let both = vec![wanted("a", ENCODE, "rtmp://x/y/z"), wanted("b", SMALL, "rtmp://x/y/z")];
    t.apply(spec(true), &both);
    let small = output_key("main", Some(SMALL), Some("copy:main:audio"));
    wait_for("the second pair", 15, || renditions.is_live("church", &small));
    assert_eq!(size(&read_for(&renditions, &small, 2)), Some((160, 90)));
    let tags = reading.join().unwrap();
    let video: Vec<u32> = tags.iter().filter(|t| t.kind == TagKind::Video && !t.sequence_header).map(|t| t.timestamp_ms).collect();
    let gap = video.windows(2).map(|w| w[1].saturating_sub(w[0])).max().unwrap_or(0);
    assert!(video.len() > 100 && gap < 200, "the first pair kept going through the change: {} frames, largest gap {gap} ms", video.len());
    let headers = tags.iter().filter(|t| t.kind == TagKind::Video && t.sequence_header).count();
    assert_eq!(headers, 1, "and was not restarted");
    drop(source);
}

#[test]
fn a_dead_destination_and_a_blocked_reader_slow_neither_the_publisher_nor_the_live_destination() {
    let got = Arc::new(AtomicUsize::new(0));
    let count = got.clone();
    let receiver = crate::restream::test_gate::listen(0, None, Arc::new(move |t: Option<MediaTag>| {
        if t.is_some_and(|t| t.kind == TagKind::Video) {
            count.fetch_add(1, Ordering::Relaxed);
        }
    }));
    let hub = Hub::new();
    let (source, slowest) = publish(&hub);
    let sends = Sends::new(hub.clone(), None);
    let live = wanted("live", ENCODE, &format!("rtmp://127.0.0.1:{}/live/x", receiver.port()));
    let dead = wanted("dead", ENCODE, "rtmp://127.0.0.1:9/live/nobody");
    sends.apply(vec![live, dead], spec(false));
    // A reader of the same pair that never reads: its queue fills and drops.
    let key = output_key("main", Some(ENCODE), Some("copy:main:audio"));
    let renditions = sends.renditions();
    wait_for("the converted pair", 15, || renditions.is_live("church", &key));
    let blocked = renditions.subscribe("church", &key);
    wait_for("the live destination to receive", 15, || got.load(Ordering::Relaxed) > 10);
    let before = got.load(Ordering::Relaxed);
    std::thread::sleep(Duration::from_secs(4));
    let sent = got.load(Ordering::Relaxed) - before;
    assert!(sent >= 100, "the live destination kept its frame rate: {sent} frames in 4 s");
    assert!(blocked.dropped_gops() > 0 || blocked.waiting().0 > 0, "the blocked reader held or lost GOPs of its own");
    let worst = slowest.load(Ordering::Relaxed);
    assert!(worst < 20_000, "the publisher never waited on a reader: slowest push {worst} µs");
    let states: Vec<String> = sends.rates().iter().map(|r| r["state"].as_str().unwrap_or("").to_string()).collect();
    assert!(states.contains(&"live".to_string()), "{states:?}");
    drop(source);
}

#[test]
fn a_node_that_cannot_be_built_fails_its_destination_with_the_reason() {
    let hub = Hub::new();
    let (source, _) = publish(&hub);
    let sends = Sends::new(hub.clone(), None);
    let mut specs = spec(false);
    specs[0].nodes[2].raw["element"] = json!("no-such-encoder");
    sends.apply(vec![wanted("yt", ENCODE, "rtmp://127.0.0.1:9/live/x")], specs);
    wait_for("the failure to show", 15, || sends.rates().first().is_some_and(|r| r["state"] == "failed"));
    drop(source);
}

#[path = "tests_hevc.rs"]
mod hevc;
