//! One guest's browser for `guest_cost`, in a process of its own: a sender
//! of 1280x720 H.264 and Opus, then a receiver that decodes the return. Each
//! offer goes out on stdout as `OFFER <sdp>` and its answer comes back on
//! stdin as `ANSWER <sdp>`, with `|` for the line ends. Runs until its
//! parent closes stdin or kills it.

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_webrtc as gst_webrtc;
use std::io::BufRead;
use std::time::{Duration, Instant};

const SENDER: &str = "webrtcbin name=w bundle-policy=max-bundle \
    videotestsrc is-live=true pattern=ball ! video/x-raw,width=1280,height=720,framerate=30/1 \
    ! x264enc tune=zerolatency speed-preset=ultrafast key-int-max=60 bframes=0 bitrate=2500 ! video/x-h264,profile=constrained-baseline \
    ! rtph264pay config-interval=-1 pt=102 ! application/x-rtp,media=video,encoding-name=H264,payload=102 ! w. \
    audiotestsrc is-live=true ! opusenc ! rtpopuspay pt=111 ! application/x-rtp,media=audio,encoding-name=OPUS,payload=111 ! w.";

pub fn run() {
    gst::init().unwrap();
    godwinmix_core::webrtc_ice::set_tcp(false);
    let send = gst::parse::launch(SENDER).unwrap().downcast::<gst::Pipeline>().unwrap();
    let sbin = send.by_name("w").unwrap();
    godwinmix_core::webrtc_ice::apply(&sbin);
    send.set_state(gst::State::Playing).unwrap();
    let recv = receiver();
    let rbin = recv.by_name("r").unwrap();
    let mut stdin = std::io::stdin().lock();
    for bin in [&sbin, &rbin] {
        println!("OFFER {}", offer(bin).replace("\r\n", "|").replace('\n', "|"));
        let mut line = String::new();
        if stdin.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let sdp = line.trim_end().strip_prefix("ANSWER ").unwrap_or("").replace('|', "\r\n");
        answer(bin, &sdp);
    }
    let mut rest = String::new();
    while stdin.read_line(&mut rest).unwrap_or(0) > 0 {}
    let _ = send.set_state(gst::State::Null);
    let _ = recv.set_state(gst::State::Null);
}

fn receiver() -> gst::Pipeline {
    let pipeline = gst::Pipeline::new();
    let bin = gst::ElementFactory::make("webrtcbin").name("r").property_from_str("bundle-policy", "max-bundle").build().unwrap();
    godwinmix_core::webrtc_ice::apply(&bin);
    pipeline.add(&bin).unwrap();
    for caps in [
        "application/x-rtp,media=video,encoding-name=H264,clock-rate=90000,payload=102,packetization-mode=(string)1",
        "application/x-rtp,media=audio,encoding-name=OPUS,clock-rate=48000,payload=111",
    ] {
        let recv = gst_webrtc::WebRTCRTPTransceiverDirection::Recvonly;
        bin.emit_by_name::<Option<gst_webrtc::WebRTCRTPTransceiver>>("add-transceiver", &[&recv, &caps.parse::<gst::Caps>().unwrap()]);
    }
    let weak = pipeline.downgrade();
    bin.connect_pad_added(move |_, pad| {
        let Some(p) = weak.upgrade() else { return };
        let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
        let enc = caps.structure(0).and_then(|s| s.get::<String>("encoding-name").ok()).unwrap_or_default();
        let chain = if enc == "H264" { "rtph264depay ! h264parse ! avdec_h264" } else { "rtpopusdepay ! opusdec" };
        let b = gst::parse::bin_from_description(&format!("{chain} ! fakesink sync=false"), true).unwrap();
        p.add(&b).unwrap();
        b.sync_state_with_parent().unwrap();
        let _ = pad.link(&b.static_pad("sink").unwrap());
    });
    pipeline.set_state(gst::State::Playing).unwrap();
    pipeline
}

fn offer(bin: &gst::Element) -> String {
    let promise = gst::Promise::new();
    bin.emit_by_name::<()>("create-offer", &[&None::<gst::Structure>, &promise]);
    promise.wait();
    let offer = promise.get_reply().unwrap().get::<gst_webrtc::WebRTCSessionDescription>("offer").unwrap();
    bin.emit_by_name::<()>("set-local-description", &[&offer, &None::<gst::Promise>]);
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until
        && bin.property::<gst_webrtc::WebRTCICEGatheringState>("ice-gathering-state") != gst_webrtc::WebRTCICEGatheringState::Complete
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    bin.property::<Option<gst_webrtc::WebRTCSessionDescription>>("local-description").unwrap().sdp().as_text().unwrap()
}

fn answer(bin: &gst::Element, sdp: &str) {
    let message = gstreamer_sdp::SDPMessage::parse_buffer(sdp.as_bytes()).expect("the answer is SDP");
    let answer = gst_webrtc::WebRTCSessionDescription::new(gst_webrtc::WebRTCSDPType::Answer, message);
    let promise = gst::Promise::new();
    bin.emit_by_name::<()>("set-remote-description", &[&answer, &promise]);
    promise.wait();
}
