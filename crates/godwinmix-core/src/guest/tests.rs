//! A guest published by a real WebRTC sender in this process: a second
//! `webrtcbin` that offers H.264 and Opus the way a browser does. ICE, DTLS
//! and SRTP all run; what crosses is read back off the inter channel the
//! guest's source reads.

use super::{Link, Shape};
use crate::preview::whep::IceConfig;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_webrtc as gst_webrtc;
use std::time::{Duration, Instant};

const SHAPE: Shape = Shape { fps: crate::config::FrameRate::whole(30), rate: 48_000, channels: 2 };

fn lan() -> IceConfig {
    IceConfig { stun: String::new(), turn: Vec::new() }
}

fn ready() -> bool {
    if gst::init().is_err() {
        return false;
    }
    let need = ["webrtcbin", "nicesrc", "x264enc", "opusenc", "avdec_h264", "intervideosrc"];
    match need.iter().find(|e| !crate::probe::exists(e)) {
        Some(e) => {
            eprintln!("skipping: {e} is not installed");
            false
        }
        None => true,
    }
}

/// The guest's browser: a 640x360 picture and a tone, sent only.
pub(super) struct Sender {
    pub pipeline: gst::Pipeline,
    bin: gst::Element,
}

pub(super) fn sender() -> Sender {
    let pipeline = gst::parse::launch(
        "webrtcbin name=w bundle-policy=max-bundle \
         videotestsrc is-live=true pattern=ball ! video/x-raw,width=640,height=360,framerate=30/1 \
         ! x264enc tune=zerolatency key-int-max=30 bframes=0 ! video/x-h264,profile=constrained-baseline \
         ! rtph264pay config-interval=-1 pt=102 ! application/x-rtp,media=video,encoding-name=H264,payload=102 ! w. \
         audiotestsrc is-live=true ! opusenc ! rtpopuspay pt=111 ! application/x-rtp,media=audio,encoding-name=OPUS,payload=111 ! w.",
    )
    .expect("the sender parses")
    .downcast::<gst::Pipeline>()
    .expect("a pipeline");
    let bin = pipeline.by_name("w").expect("webrtcbin");
    crate::webrtc_ice::set_tcp(false);
    crate::webrtc_ice::apply(&bin);
    pipeline.set_state(gst::State::Playing).expect("the sender plays");
    Sender { pipeline, bin }
}

impl Sender {
    pub fn offer(&self) -> String {
        let promise = gst::Promise::new();
        self.bin.emit_by_name::<()>("create-offer", &[&None::<gst::Structure>, &promise]);
        promise.wait();
        let offer = promise.get_reply().unwrap().get::<gst_webrtc::WebRTCSessionDescription>("offer").unwrap();
        self.bin.emit_by_name::<()>("set-local-description", &[&offer, &None::<gst::Promise>]);
        let until = Instant::now() + Duration::from_secs(5);
        while Instant::now() < until
            && self.bin.property::<gst_webrtc::WebRTCICEGatheringState>("ice-gathering-state")
                != gst_webrtc::WebRTCICEGatheringState::Complete
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        let local = self.bin.property::<Option<gst_webrtc::WebRTCSessionDescription>>("local-description").unwrap();
        local.sdp().as_text().unwrap()
    }

    pub fn answer(&self, sdp: &str) {
        let message = gstreamer_sdp::SDPMessage::parse_buffer(sdp.as_bytes()).expect("the answer is SDP");
        let answer = gst_webrtc::WebRTCSessionDescription::new(gst_webrtc::WebRTCSDPType::Answer, message);
        let promise = gst::Promise::new();
        self.bin.emit_by_name::<()>("set-remote-description", &[&answer, &promise]);
        promise.wait();
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

/// Whether a frame of the sender's own size comes off the guest's channel
/// within `secs`: the inter source makes 320x240 black on its own, so a
/// 640x360 frame can only be the guest's.
fn guest_frame_within(channel: &str, secs: u64) -> bool {
    let reader = gst::parse::launch(&format!("intervideosrc channel={channel} ! appsink name=out sync=false max-buffers=2 drop=true"))
        .unwrap()
        .downcast::<gst::Pipeline>()
        .unwrap();
    let out = reader.by_name("out").unwrap();
    reader.set_state(gst::State::Playing).unwrap();
    let until = Instant::now() + Duration::from_secs(secs);
    let mut seen = false;
    while !seen && Instant::now() < until {
        let sample = out.emit_by_name::<Option<gst::Sample>>("try-pull-sample", &[&100_000_000u64]);
        let width = sample.and_then(|s| s.caps().and_then(|c| c.structure(0).and_then(|st| st.get::<i32>("width").ok())));
        seen = width == Some(640);
    }
    let _ = reader.set_state(gst::State::Null);
    seen
}

fn wait_for(id: &str, want: Link, secs: u64) -> Link {
    let until = Instant::now() + Duration::from_secs(secs);
    while super::link(id) != want && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
    super::link(id)
}

#[test]
fn a_guest_offer_is_answered_and_its_picture_reaches_the_channel() {
    if !ready() {
        return;
    }
    let s = sender();
    let (session, answer) = super::offer("test-ana", &s.offer(), SHAPE, &lan()).expect("the offer is answered");
    assert!(answer.contains("a=recvonly"), "the answer only receives:\n{answer}");
    assert!(answer.contains("H264"), "and takes H.264:\n{answer}");
    s.answer(&answer);
    assert_eq!(wait_for("test-ana", Link::Live, 15), Link::Live);
    assert!(guest_frame_within(&super::channel("test-ana"), 10), "the guest's 640x360 picture reached the channel");
    assert!(super::end("test-ana", &session), "DELETE ends the session");
    assert_eq!(super::link("test-ana"), Link::Gone);
    assert!(!super::end("test-ana", &session), "and only once");
    super::hang_up("test-ana");
    assert_eq!(super::link("test-ana"), Link::Waiting, "a guest hung up is forgotten");
}

#[test]
fn an_offer_with_no_picture_codec_or_no_sdp_is_refused_by_status() {
    if !ready() {
        return;
    }
    let bad = super::offer("test-bad", "hello", SHAPE, &lan()).expect_err("not SDP");
    assert_eq!(bad.status, 400);
    let av1 = "v=0\r\no=- 0 0 IN IP4 0.0.0.0\r\ns=-\r\nt=0 0\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\na=rtpmap:96 AV1/90000\r\n";
    let refused = super::offer("test-bad", av1, SHAPE, &lan()).expect_err("AV1 alone");
    assert_eq!(refused.status, 415);
    assert!(refused.message.contains("H.264"), "{}", refused.message);
    super::hang_up("test-bad");
}
