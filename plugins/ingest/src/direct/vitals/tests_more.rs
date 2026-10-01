//! The input going away, stalling, and a show nobody asked anything of.

use std::time::Duration;

use gstreamer as gst;
use gstreamer::prelude::*;

use super::{publish, watch, MOVING};
use crate::hub::Hub;

#[test]
fn a_show_with_no_publisher_has_no_input_and_gets_it_when_one_comes() {
    let hub = Hub::new();
    let w = watch(&hub, true, false);
    let e = w.until(&["no-input"], Duration::from_secs(5));
    assert_eq!(e["params"]["show"], "news");
    let _p = publish(&hub, MOVING, "wave=sine");
    w.until(&[], Duration::from_secs(10));
}

#[test]
fn a_publisher_that_stops_sending_is_a_stall_until_it_sends_again() {
    let hub = Hub::new();
    let w = watch(&hub, true, false);
    let p = publish(&hub, MOVING, "wave=sine");
    w.until(&[], Duration::from_secs(10));
    w.clear();
    p.set_state(gst::State::Paused).unwrap();
    w.until(&["stall"], Duration::from_secs(10));
    w.clear();
    p.set_state(gst::State::Playing).unwrap();
    w.until(&[], Duration::from_secs(10));
}

#[test]
fn with_its_alarms_off_and_nobody_looking_nothing_is_decoded() {
    let hub = Hub::new();
    let w = watch(&hub, false, false);
    let _p = publish(&hub, "pattern=black", "wave=silence");
    w.until(&[], Duration::from_secs(10));
    std::thread::sleep(Duration::from_secs(4));
    assert_eq!(w.vitals.counts(), (0, 0), "no job was made, so nothing was decoded");
    let all = w.events.lock().unwrap().clone();
    assert!(all.iter().all(|e| super::kinds(e).iter().all(|k| k == "no-input")), "{all:#?}");
    // A thumbnail request turns pictures on for a while, alarms or not.
    assert!(w.vitals.thumbnail("news", Some(160)).unwrap().is_none());
    std::thread::sleep(Duration::from_secs(3));
    assert!(w.vitals.thumbnail("news", Some(160)).unwrap().is_some(), "the request started the keyframe decode");
}
