use super::*;
use crate::gstutil::{make, queue_thread};
use gstreamer as gst;
use gstreamer::prelude::*;

fn held(element: &str, mb: u64, source: Option<&str>) -> Held {
    Held { pipeline: "programme".into(), element: element.into(), bytes: mb * MB, source: source.map(str::to_string) }
}

#[test]
fn the_fullest_queue_with_a_source_is_blamed_once_it_holds_enough() {
    let queues = [held("venc-q", 900, None), held("pgm-vq-cam-a", 700, Some("cam-a")), held("slot-q-1", 80, Some("cam-b"))];
    let (q, source) = blame(&queues).expect("a source to blame");
    assert_eq!((q.element.as_str(), source), ("pgm-vq-cam-a", "cam-a"), "the encoder is never restarted");
    let small = [held("pgm-vq-cam-a", 10, Some("cam-a"))];
    assert!(blame(&small).is_none(), "ten megabytes is a normal queue, not the culprit");
    let nobody = [held("venc-q", 900, None)];
    assert!(blame(&nobody).is_none());
}

#[test]
fn a_queue_is_traced_to_its_source_by_pipeline_or_by_the_chain_above_it() {
    let _ = gst::init();
    let input = gst::Pipeline::with_name("input-cam-a-main");
    let q = queue_thread("cam-a-main-vprog-q").unwrap();
    input.add(&q).unwrap();
    assert_eq!(source_of("input-cam-a-main", &q).as_deref(), Some("cam-a-main"));

    // pgm-vq-<id> ! pgm-vtee-<id> ! slot-gate-0 ! slot-q-0, as the programme has it.
    let programme = gst::Pipeline::with_name("programme");
    let vq = queue_thread("pgm-vq-cam-b").unwrap();
    let tee = make("tee", "pgm-vtee-cam-b").unwrap();
    let gate = make("valve", "slot-gate-0").unwrap();
    let slot = queue_thread("slot-q-0").unwrap();
    let venc = queue_thread("venc-q").unwrap();
    programme.add_many([&vq, &tee, &gate, &slot, &venc]).unwrap();
    gst::Element::link_many([&vq, &tee, &gate, &slot]).unwrap();
    assert_eq!(source_of("programme", &vq).as_deref(), Some("cam-b"));
    assert_eq!(source_of("programme", &slot).as_deref(), Some("cam-b"), "a slot hangs off a source's tee");
    assert_eq!(source_of("programme", &venc), None, "the encoder's queue is nobody's source");
    let output = gst::Pipeline::with_name("output-yt");
    let mux_q = queue_thread("out-yt-mux-vq-0").unwrap();
    output.add(&mux_q).unwrap();
    assert_eq!(source_of("output-yt", &mux_q), None);
}

#[test]
fn the_log_names_the_fullest_queues_and_says_so_when_there_are_none() {
    let queues = [held("pgm-vq-cam-a", 700, Some("cam-a")), held("venc-q", 3, None)];
    assert_eq!(describe(&queues), "pgm-vq-cam-a in programme: 700.0 MB, venc-q in programme: 3.0 MB");
    assert_eq!(describe(&[]), "no queue holds anything");
}
