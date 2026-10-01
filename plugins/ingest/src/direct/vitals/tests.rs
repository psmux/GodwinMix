//! The vitals against real streams: an H.264 and AAC publisher on the hub,
//! made black, frozen, silent or stalled on purpose, and the events that
//! come out. Each test turns the fault on, waits for exactly its alarm, turns
//! the fault off and waits for the show to be well again.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::{json, Value};

use super::Vitals;
use crate::hub::{Hub, Publication};
use crate::media_tag::MediaTag;
use crate::rtmp::Inlet;
use crate::tagger;

struct Push(Publication);

impl Inlet for Push {
    fn tag(&mut self, tag: MediaTag) {
        self.0.push(tag);
    }
}

/// A live 640x360 H.264 and AAC publisher on `hub` as `news/main`, a
/// keyframe a second, from a test picture and a test tone.
fn publish(hub: &Hub, pattern: &str, wave: &str) -> gst::Pipeline {
    gmx_netkit::init().unwrap();
    let to = tagger::share(Box::new(Push(hub.publish("news", "main", "127.0.0.1:1", None).unwrap())));
    let zero = Arc::new(tagger::Zero::default());
    let line = format!(
        "videotestsrc name=v is-live=true {pattern} ! video/x-raw,width=640,height=360,framerate=30/1 \
         ! x264enc tune=zerolatency speed-preset=ultrafast key-int-max=30 bitrate=800 ! h264parse name=vp \
         audiotestsrc name=a is-live=true {wave} ! audio/x-raw,rate=48000,channels=2 ! avenc_aac ! aacparse name=ap"
    );
    let pipeline = gst::parse::launch(&line).unwrap().downcast::<gst::Pipeline>().unwrap();
    for (parser, sink) in [("vp", tagger::video_sink(to.clone(), zero.clone())), ("ap", tagger::audio_sink(to, zero))] {
        pipeline.add(&sink).unwrap();
        pipeline.by_name(parser).unwrap().link(&sink).unwrap();
    }
    pipeline.set_state(gst::State::Playing).unwrap();
    pipeline
}

const MOVING: &str = "pattern=smpte75 horizontal-speed=8";

struct Watch {
    vitals: Arc<Vitals>,
    events: Arc<Mutex<Vec<Value>>>,
}

fn watch(hub: &Hub, alarms: bool, pictures: bool) -> Watch {
    let events: Arc<Mutex<Vec<Value>>> = Arc::default();
    let sink = events.clone();
    let vitals = Vitals::start(hub.clone(), Arc::new(move |name, v| sink.lock().unwrap().push(json!({"name": name, "params": v}))), 1);
    let thresholds = json!({"black_secs": 2, "freeze_secs": 3, "silence_secs": 2, "stall_secs": 2});
    vitals.watch("news", "news", "main", &json!({"alarms": alarms, "pictures": pictures, "thresholds": thresholds}));
    Watch { vitals, events }
}

fn kinds(event: &Value) -> Vec<String> {
    let alarms = event["params"]["health"]["alarms"].as_array().cloned().unwrap_or_default();
    alarms.iter().map(|a| a["kind"].as_str().unwrap_or("").to_string()).collect()
}

impl Watch {
    /// Wait for a health event whose alarm kinds are exactly `want`.
    fn until(&self, want: &[&str], limit: Duration) -> Value {
        let deadline = Instant::now() + limit;
        while Instant::now() < deadline {
            let found = self.events.lock().unwrap().iter().rev().find(|e| kinds(e) == want).cloned();
            if let Some(e) = found {
                return e;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("no health event with {want:?} in {limit:?}; saw {:#?}", self.events.lock().unwrap());
    }

    fn clear(&self) {
        self.events.lock().unwrap().clear();
    }
}

#[test]
fn a_clean_stream_raises_nothing_and_gets_a_thumbnail() {
    let hub = Hub::new();
    let w = watch(&hub, true, true);
    let _p = publish(&hub, MOVING, "wave=sine");
    let ok = w.until(&[], Duration::from_secs(10));
    assert_eq!(ok["params"]["health"]["state"], "ok");
    std::thread::sleep(Duration::from_secs(5));
    let all = w.events.lock().unwrap().clone();
    assert!(all.iter().all(|e| kinds(e).is_empty() || kinds(e) == ["no-input"]), "{all:#?}");
    let thumb = w.vitals.thumbnail("news").unwrap().expect("a thumbnail after five seconds");
    assert_eq!((thumb.width, thumb.height), (320, 180));
    assert_eq!(&thumb.jpeg[..2], &[0xff, 0xd8], "a JPEG");
}

#[test]
fn a_black_picture_is_black_until_it_is_not() {
    let hub = Hub::new();
    let w = watch(&hub, true, false);
    let p = publish(&hub, "pattern=black", "wave=sine");
    let e = w.until(&["black"], Duration::from_secs(15));
    assert_eq!(e["params"]["health"]["state"], "alarm");
    w.clear();
    p.by_name("v").unwrap().set_property_from_str("pattern", "smpte75");
    p.by_name("v").unwrap().set_property("horizontal-speed", 8i32);
    w.until(&[], Duration::from_secs(10));
    assert!(w.vitals.thumbnail("news").unwrap().is_none(), "no JPEG was made: nobody was looking");
}

#[test]
fn a_still_picture_freezes_until_it_moves() {
    let hub = Hub::new();
    let w = watch(&hub, true, false);
    let p = publish(&hub, "pattern=smpte75", "wave=sine");
    w.until(&["freeze"], Duration::from_secs(20));
    w.clear();
    p.by_name("v").unwrap().set_property("horizontal-speed", 8i32);
    w.until(&[], Duration::from_secs(10));
}

#[test]
fn quiet_sound_is_silence_until_the_tone_comes_back() {
    let hub = Hub::new();
    let w = watch(&hub, true, false);
    let p = publish(&hub, MOVING, "wave=silence");
    w.until(&["silence"], Duration::from_secs(15));
    w.clear();
    p.by_name("a").unwrap().set_property_from_str("wave", "sine");
    w.until(&[], Duration::from_secs(10));
}

#[path = "tests_more.rs"]
mod more;
