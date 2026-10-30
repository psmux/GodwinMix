//! A guest's browser, in this process: a `webrtcbin` that sends a picture and
//! a tone, and one that receives the return and counts what it decodes. ICE,
//! DTLS and SRTP all run over loopback.

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_webrtc as gst_webrtc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub fn ready() -> bool {
    let _ = gst::init();
    let need = ["webrtcbin", "nicesrc", "x264enc", "opusenc", "avdec_h264", "opusdec", "intervideosrc", "avenc_aac"];
    match need.iter().find(|e| gst::ElementFactory::find(e).is_none()) {
        Some(e) => {
            eprintln!("skipping: {e} is not installed");
            false
        }
        None => true,
    }
}

pub struct Peer {
    pipeline: gst::Pipeline,
    bin: gst::Element,
    pub frames: Arc<AtomicU64>,
    pub sound: Arc<AtomicU64>,
}

/// Sends 640x360 H.264 and an Opus tone, as a browser's publisher does.
pub fn sender() -> Peer {
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
    godwinmix_core::webrtc_ice::set_tcp(false);
    godwinmix_core::webrtc_ice::apply(&bin);
    pipeline.set_state(gst::State::Playing).expect("the sender plays");
    Peer { pipeline, bin, frames: Default::default(), sound: Default::default() }
}

/// Receives H.264 and Opus, decodes and counts them, as the return player does.
pub fn viewer() -> Peer {
    let pipeline = gst::Pipeline::new();
    let bin = gst::ElementFactory::make("webrtcbin").property_from_str("bundle-policy", "max-bundle").build().unwrap();
    godwinmix_core::webrtc_ice::set_tcp(false);
    godwinmix_core::webrtc_ice::apply(&bin);
    pipeline.add(&bin).unwrap();
    let wanted = [
        "application/x-rtp,media=video,encoding-name=H264,clock-rate=90000,payload=102,packetization-mode=(string)1",
        "application/x-rtp,media=audio,encoding-name=OPUS,clock-rate=48000,payload=111",
    ];
    for caps in wanted {
        let caps: gst::Caps = caps.parse().unwrap();
        let recv = gst_webrtc::WebRTCRTPTransceiverDirection::Recvonly;
        bin.emit_by_name::<Option<gst_webrtc::WebRTCRTPTransceiver>>("add-transceiver", &[&recv, &caps]);
    }
    let peer = Peer { pipeline: pipeline.clone(), bin: bin.clone(), frames: Default::default(), sound: Default::default() };
    let (weak, f, s) = (pipeline.downgrade(), peer.frames.clone(), peer.sound.clone());
    bin.connect_pad_added(move |_, pad| {
        let Some(p) = weak.upgrade() else { return };
        let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
        let encoding = caps.structure(0).and_then(|s| s.get::<String>("encoding-name").ok()).unwrap_or_default();
        let (chain, count) = match encoding.as_str() {
            "H264" => ("rtph264depay ! h264parse ! avdec_h264", f.clone()),
            "OPUS" => ("rtpopusdepay ! opusdec", s.clone()),
            _ => return,
        };
        let bin = gst::parse::bin_from_description(&format!("{chain} ! fakesink sync=false name=end"), true).unwrap();
        p.add(&bin).unwrap();
        bin.sync_state_with_parent().unwrap();
        pad.link(&bin.static_pad("sink").unwrap()).unwrap();
        let end = bin.by_name("end").unwrap().static_pad("sink").unwrap();
        end.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
            count.fetch_add(1, Ordering::Relaxed);
            gst::PadProbeReturn::Ok
        });
    });
    pipeline.set_state(gst::State::Playing).unwrap();
    peer
}

impl Peer {
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

    /// Wait until both counts reach these, or `secs` pass; answer the counts.
    pub async fn wait_for(&self, frames: u64, sound: u64, secs: u64) -> (u64, u64) {
        let until = Instant::now() + Duration::from_secs(secs);
        loop {
            let got = (self.frames.load(Ordering::Relaxed), self.sound.load(Ordering::Relaxed));
            if (got.0 >= frames && got.1 >= sound) || Instant::now() > until {
                return got;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}
