//! One guest's WebRTC session: a pipeline of its own around a `webrtcbin`
//! that only receives, the offer in and the answer out.
//!
//! The answer carries one video codec, H.264 whenever the offer has it and
//! VP8 otherwise, and Opus for the sound. What arrives is decoded and handed
//! to the guest's source across an inter channel (see `branch.rs`).

use super::{branch, watch, Refusal, Shape};
use crate::preview::whep::IceConfig;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_sdp as gst_sdp;
use gstreamer_webrtc as gst_webrtc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long an answer waits for ICE to gather its candidates.
const GATHER: Duration = Duration::from_secs(3);

/// What has to be installed for a guest to get in.
pub const NEEDED: &[&str] = &["webrtcbin", "nicesrc", "rtpopusdepay", "opusdec", "intervideosink", "interaudiosink"];

pub struct Session {
    pipeline: gst::Pipeline,
    stop: Arc<AtomicBool>,
}

impl Session {
    /// Answer `offer` for the guest `id`, as its session `name`.
    pub fn start(id: &str, name: &str, offer: &str, shape: Shape, ice: &IceConfig) -> Result<(Session, String), Refusal> {
        let missing = crate::setup::system::absent(NEEDED);
        if !missing.is_empty() {
            let what = "Taking a guest's camera in over WebRTC";
            let e = crate::setup::system::missing_error(what, crate::setup::system::WEBRTC, &missing);
            return Err(Refusal::unavailable(format!("{e:#}")));
        }
        let not_sdp = || {
            Refusal::bad("the body is not SDP. POST the RTCPeerConnection's offer, with Content-Type: application/sdp.")
        };
        if !offer.trim_start().starts_with("v=") {
            return Err(not_sdp());
        }
        let sdp = gst_sdp::SDPMessage::parse_buffer(offer.as_bytes()).map_err(|_| not_sdp())?;
        let video = branch::video_codec(offer).ok_or_else(|| {
            Refusal::new(415, "the offer has neither H.264 nor VP8 video, which is what a guest's camera is taken in. Chrome, Edge, Safari and Firefox all send one of them.")
        })?;
        let pipeline = gst::Pipeline::with_name(&format!("guest-{id}-{name}"));
        let bin = webrtcbin(&pipeline, video, ice)?;
        let (weak, channel) = (pipeline.downgrade(), super::channel(id));
        bin.connect_pad_added(move |_, pad| {
            if let Some(p) = weak.upgrade() {
                branch::attach(&p, pad, &channel, shape);
            }
        });
        let stop = Arc::new(AtomicBool::new(false));
        let session = Session { pipeline: pipeline.clone(), stop: stop.clone() };
        pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| Refusal::internal(format!("the guest's session would not start: {e}")))?;
        let answer = negotiate(&bin, sdp)?;
        watch::watch(id, name, bin, stop);
        Ok((session, answer))
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

fn webrtcbin(pipeline: &gst::Pipeline, video: &str, ice: &IceConfig) -> Result<gst::Element, Refusal> {
    let bin = crate::gstutil::make("webrtcbin", &format!("{}-webrtc", pipeline.name()))
        .map_err(|e| Refusal::unavailable(format!("{e:#}")))?;
    crate::probe::set_enum(&bin, "bundle-policy", "max-bundle");
    crate::probe::set_int(&bin, "latency", 200);
    crate::webrtc_ice::apply(&bin);
    if !ice.stun.is_empty() {
        bin.set_property("stun-server", &ice.stun);
    }
    for t in &ice.turn {
        let _ = bin.emit_by_name::<bool>("add-turn-server", &[t]);
    }
    pipeline.add(&bin).map_err(|e| Refusal::internal(format!("could not add the guest's webrtcbin: {e}")))?;
    let wanted = [branch::video_caps(video), "application/x-rtp,media=audio,encoding-name=OPUS,clock-rate=48000".to_string()];
    for caps in wanted {
        let caps: gst::Caps = caps.parse().map_err(|_| Refusal::internal("the receive caps did not parse"))?;
        let recv = gst_webrtc::WebRTCRTPTransceiverDirection::Recvonly;
        let _ = bin.emit_by_name::<Option<gst_webrtc::WebRTCRTPTransceiver>>("add-transceiver", &[&recv, &caps]);
    }
    Ok(bin)
}

/// Offer in, answer out, once ICE has its candidates.
fn negotiate(bin: &gst::Element, sdp: gst_sdp::SDPMessage) -> Result<String, Refusal> {
    let offer = gst_webrtc::WebRTCSessionDescription::new(gst_webrtc::WebRTCSDPType::Offer, sdp);
    let promise = gst::Promise::new();
    bin.emit_by_name::<()>("set-remote-description", &[&offer, &promise]);
    promise.wait();
    let promise = gst::Promise::new();
    bin.emit_by_name::<()>("create-answer", &[&None::<gst::Structure>, &promise]);
    promise.wait();
    let answer = promise
        .get_reply()
        .and_then(|r| r.get::<gst_webrtc::WebRTCSessionDescription>("answer").ok())
        .ok_or_else(|| Refusal::new(406, "the mixer made no answer to that offer. Offer sendonly H.264 or VP8 video and Opus audio."))?;
    let promise = gst::Promise::new();
    bin.emit_by_name::<()>("set-local-description", &[&answer, &promise]);
    promise.wait();
    let until = Instant::now() + GATHER;
    while Instant::now() < until {
        if bin.property::<gst_webrtc::WebRTCICEGatheringState>("ice-gathering-state") == gst_webrtc::WebRTCICEGatheringState::Complete {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let local = bin.property::<Option<gst_webrtc::WebRTCSessionDescription>>("local-description").unwrap_or(answer);
    local.sdp().as_text().map_err(|e| Refusal::internal(format!("the answer would not print as SDP: {e}")))
}
