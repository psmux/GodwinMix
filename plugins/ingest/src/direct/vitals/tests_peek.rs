//! A channel stream's picture: made on the first ask from a live test feed,
//! kept while asked for, and let go once the asks stop.

use std::sync::Arc;
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::json;

use super::{now_ms, Vitals};
use crate::hub::Hub;

#[test]
fn a_live_channel_stream_gives_a_picture_and_the_tap_goes_after_the_linger() {
    let hub = Hub::new();
    let feed = super::tests::publish(&hub, "pattern=smpte75", "wave=sine");
    let vitals = Vitals::start(hub.clone(), Arc::new(|_, _| {}), 1);
    assert_eq!(vitals.peeking(), 0, "nothing is looked at before an ask");
    let ask = || vitals.call("channel.thumbnail", &json!({"app": "news", "stream": "main", "width": 320})).unwrap();
    assert_eq!(ask()["pending"], true, "the first ask starts the pictures");
    assert_eq!(vitals.peeking(), 1);
    let until = Instant::now() + Duration::from_secs(15);
    let answer = loop {
        let a = ask();
        if a["jpeg"].is_string() {
            break a;
        }
        assert!(Instant::now() < until, "no picture of news/main in 15 s: {a}");
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!((answer["width"].as_u64(), answer["height"].as_u64()), (Some(320), Some(180)), "{answer}");
    assert!(answer["jpeg"].as_str().unwrap().starts_with("/9j/"), "a base64 JPEG");
    // A show's table does not take a channel stream's tap away.
    vitals.keep(&[]);
    assert_eq!(vitals.peeking(), 1);
    // Ten seconds after the last ask the tap and its hub reader go.
    vitals.tick(now_ms() + super::registry::ASKED_FOR_MS + 1_000);
    assert_eq!(vitals.peeking(), 0, "the tap outlived its linger");
    let _ = feed.set_state(gst::State::Null);
}

#[test]
fn a_stream_nobody_publishes_is_refused_and_watched_by_nothing() {
    let hub = Hub::new();
    let vitals = Vitals::start(hub, Arc::new(|_, _| {}), 1);
    let a = vitals.call("channel.thumbnail", &json!({"app": "quiet", "stream": "main"})).unwrap();
    assert_eq!(a["status"], 404, "{a}");
    assert!(a["why"].as_str().unwrap().contains("nothing is publishing to quiet/main"), "{a}");
    assert_eq!(vitals.peeking(), 0);
}
