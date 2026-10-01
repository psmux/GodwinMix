//! The vitals inside the direct host: a row with its alarms on raises
//! `direct.health`, `direct.thumbnail` answers a JPEG at the wall's width,
//! and frames from a decode that already exists are used in place of a
//! keyframe decode.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::{json, Value};

use super::Vitals;
use crate::direct::{Emit, Host, Relay};
use crate::hub::Hub;

type Heard = Arc<Mutex<Vec<(String, Value)>>>;

fn wait_for(what: &str, secs: u64, mut ok: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(secs);
    while !ok() {
        assert!(Instant::now() < until, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_direct_show_with_alarms_on_says_it_went_black_and_serves_its_picture() {
    let hub = Hub::new();
    let heard: Heard = Arc::default();
    let h = heard.clone();
    let emit: Emit = Arc::new(move |name, params| h.lock().unwrap().push((name.to_string(), params)));
    let relay: Relay = Arc::new(|| "127.0.0.1:1935".to_string());
    let host = Host::new(hub.clone(), emit, relay);
    // The feed arrives as a channel stream; `news/main` is what the tests
    // beside this one publish.
    let feed = super::tests::publish(&hub, "pattern=black", "wave=sine");
    let monitor = json!({"alarms": true, "pictures": false, "thresholds": {"black_secs": 2}});
    host.apply(&json!({"direct": [{"id": "bbc-one", "input": {"uri": "channel:news/main"}, "outputs": [], "monitor": monitor}]}));
    let black = |heard: &Heard| {
        let h = heard.lock().unwrap();
        h.iter().any(|(n, v)| n == "direct.health" && v["show"] == "bbc-one" && v["health"]["alarms"][0]["kind"] == "black")
    };
    wait_for("direct.health with black", 20, || black(&heard));
    let thumb = || host.call("direct.thumbnail", &json!({"show": "bbc-one", "width": 160})).unwrap();
    assert_eq!(thumb()["pending"], true, "the first ask starts the pictures");
    wait_for("a thumbnail", 10, || thumb()["jpeg"].is_string());
    let t = thumb();
    assert_eq!((t["width"].as_u64(), t["height"].as_u64()), (Some(160), Some(90)), "{t}");
    assert!(t["jpeg"].as_str().unwrap().starts_with("/9j/"), "a base64 JPEG");
    assert_eq!(host.call("direct.thumbnail", &json!({"show": "nobody"})).unwrap()["status"], 404);
    let _ = feed.set_state(gst::State::Null);
}

#[test]
fn frames_from_an_existing_decode_make_the_picture_with_no_keyframe_decoded() {
    gmx_netkit::init().unwrap();
    let hub = Hub::new();
    let vitals = Vitals::start(hub.clone(), Arc::new(|_, _| {}), 1);
    // Nothing publishes `quiet/main`: every picture has to come from the
    // frames offered, as a show that converts hands them over.
    vitals.watch("converted", "quiet", "main", &json!({"alarms": true, "pictures": true}));
    let raw = gst::parse::launch("videotestsrc num-buffers=3 pattern=smpte ! video/x-raw,format=I420,width=1280,height=720 ! appsink name=s")
        .unwrap()
        .downcast::<gst::Pipeline>()
        .unwrap();
    let sink = raw.by_name("s").unwrap().downcast::<gstreamer_app::AppSink>().unwrap();
    raw.set_state(gst::State::Playing).unwrap();
    let frame = sink.pull_sample().unwrap();
    vitals.offer_frame("converted", &frame);
    wait_for("a thumbnail from the offered frame", 10, || vitals.thumbnail("converted", None).unwrap().is_some());
    let t = vitals.thumbnail("converted", None).unwrap().unwrap();
    assert_eq!((t.width, t.height), (320, 180));
    let _ = raw.set_state(gst::State::Null);
}
