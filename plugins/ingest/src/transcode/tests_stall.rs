//! A converted stream whose encoder fails: built again, and never holding
//! up the thread that would build it.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSrc;

use super::*;
use crate::media_tag::{MediaTag, TagKind};
use crate::transcode::input::Input;

/// Every encoder in the stream's pipeline now, by name.
fn encoders(t: &Transcoders) -> Vec<gst::Element> {
    let Some(p) = t.pipeline("church", "main") else { return Vec::new() };
    p.iterate_elements().into_iter().flatten().filter(|e| e.name().ends_with("-x264enc")).collect()
}

/// Make `enc` fail as NVENC does when it has no session: an error on the bus
/// and an error back up its stream, which stops the decode in front of it.
fn fail(enc: &gst::Element) {
    let pad = enc.static_pad("sink").unwrap();
    pad.add_probe(gst::PadProbeType::BUFFER, |_, info| {
        info.flow_res = Err(gst::FlowError::Error);
        gst::PadProbeReturn::Handled
    });
    gst::element_error!(enc, gst::StreamError::Encode, ["Could not encode stream."]);
}

/// The encoder fails, and the one built in its place fails again at once,
/// as an encoder did for one channel of fifty started together. The second
/// failure came inside the wait between two builds of a node, its error was
/// taken off the bus and dropped, the node was never built again, and the
/// pair sent nothing until the stream was stopped.
#[test]
fn an_encoder_that_fails_again_at_once_is_built_again_and_its_pair_sends() {
    let hub = Hub::new();
    let (source, _) = publish(&hub);
    let t = Transcoders::new(hub.clone());
    t.apply(spec(false), &[wanted("a", ENCODE, "rtmp://x/y/z")]);
    let key = output_key("main", Some(ENCODE), Some("copy:main:audio"));
    let renditions = t.renditions();
    wait_for("the converted pair", 15, || renditions.is_live("church", &key));
    let mut failed: Vec<gst::Element> = Vec::new();
    for round in 0..2 {
        let mut enc = None;
        wait_for(&format!("encoder {round}"), 15, || {
            enc = encoders(&t).into_iter().find(|e| !failed.contains(e));
            enc.is_some()
        });
        let enc = enc.unwrap();
        fail(&enc);
        failed.push(enc);
    }
    // Built again once the wait is over, and sending: frames read well
    // after both failures.
    let mut frames = 0;
    let until = Instant::now() + Duration::from_secs(30);
    while frames < 30 && Instant::now() < until {
        frames += read_for(&renditions, &key, 2).iter().filter(|t| t.kind == TagKind::Video && !t.sequence_header).count();
    }
    assert!(frames >= 30, "the pair sends again after its encoder failed twice: {frames} frames; encoders now {:?}", encoders(&t));
    drop(source);
}

/// A decoder that takes nothing more fills its appsrc. Pushing to it must
/// not wait, or the session's thread stops with it.
#[test]
fn a_decoder_that_takes_nothing_never_holds_up_the_push() {
    gmx_netkit::init().unwrap();
    let src = AppSrc::builder().format(gst::Format::Time).is_live(true).block(true).max_bytes(4096).build();
    let sink = gst::ElementFactory::make("fakesink").build().unwrap();
    let pipeline = gst::Pipeline::new();
    pipeline.add_many([src.upcast_ref(), &sink]).unwrap();
    src.link(&sink).unwrap();
    // Its stream stops at the first buffer, as after a failed node.
    sink.static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, |_, info| {
        info.flow_res = Err(gst::FlowError::Error);
        gst::PadProbeReturn::Handled
    });
    pipeline.set_state(gst::State::Playing).unwrap();
    let input = Arc::new(Input::default());
    let header = MediaTag { track: 0, kind: TagKind::Video, timestamp_ms: 0, keyframe: true, sequence_header: true, payload: Arc::from(vec![0x17, 0, 0, 0, 0]) };
    input.push(&header, 0);
    input.attach(TagKind::Video, 0, Some(src.clone()));
    let pushed = Arc::new(AtomicUsize::new(0));
    let (i, n) = (input.clone(), pushed.clone());
    let pushing = std::thread::spawn(move || {
        for k in 0..200u32 {
            let mut body = vec![if k % 10 == 0 { 0x17 } else { 0x27 }, 1, 0, 0, 0];
            body.resize(1024, 0);
            let tag = MediaTag { track: 0, kind: TagKind::Video, timestamp_ms: k * 33, keyframe: k % 10 == 0, sequence_header: false, payload: Arc::from(body) };
            i.push(&tag, 0);
            n.fetch_add(1, Ordering::Relaxed);
        }
    });
    let until = Instant::now() + Duration::from_secs(10);
    while !pushing.is_finished() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    let done = pushed.load(Ordering::Relaxed);
    let _ = pipeline.set_state(gst::State::Null);
    assert_eq!(done, 200, "every push came back, the full appsrc skipped");
    assert!(src.current_level_bytes() <= 4096 + 1024, "and nothing was queued past its limit: {}", src.current_level_bytes());
}
