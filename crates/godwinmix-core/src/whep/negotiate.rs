//! The WebRTC half of a viewer: the webrtcbin, which offer section each pad
//! answers, the codec it prefers, and the answer once the candidates are in.

use super::params::WhepParams;
use super::sdp;
use super::Refusal;
use crate::gstutil::make;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_webrtc as gst_webrtc;
use std::time::{Duration, Instant};

/// How long an answer waits for ICE to gather its candidates.
const GATHER: Duration = Duration::from_secs(3);

/// One tee to one offer section: what it carries and as which payload type.
pub struct Branch<'a> {
    pub tee: &'a gst::Element,
    pub media: &'static str,
    pub payloader: &'static str,
    pub encoding: &'static str,
    pub pt: u8,
}

pub fn webrtcbin(name: &str, params: &WhepParams) -> Result<gst::Element, Refusal> {
    let bin = make("webrtcbin", name).map_err(|e| Refusal::unavailable(format!("{e:#}")))?;
    crate::probe::set_enum(&bin, "bundle-policy", "max-bundle");
    if !params.stun.is_empty() {
        bin.set_property("stun-server", &params.stun);
    }
    for t in &params.turn {
        let _ = bin.emit_by_name::<bool>("add-turn-server", &[t]);
    }
    Ok(bin)
}

/// The index of the first `m=<media>` section, which names webrtcbin's pad.
pub fn mline(offer: &str, media: &str) -> Option<usize> {
    offer
        .lines()
        .filter(|l| l.starts_with("m="))
        .position(|l| l[2..].split_whitespace().next() == Some(media))
}

/// Say which of the offer's formats this transceiver sends, and that it only
/// sends. Without the preference webrtcbin would answer from the payloader's
/// template caps, which name every profile at once.
///
/// Done before the pad is asked for: the offer made this transceiver, and
/// webrtcbin gives no sink pad to a transceiver that only receives.
pub fn prefer(bin: &gst::Element, index: usize, offer: &str, b: &Branch<'_>) {
    let index = i32::try_from(index).unwrap_or(0);
    let Some(transceiver) = bin.emit_by_name::<Option<gst_webrtc::WebRTCRTPTransceiver>>("get-transceiver", &[&index])
    else {
        return;
    };
    transceiver.set_property("direction", gst_webrtc::WebRTCRTPTransceiverDirection::Sendonly);
    let fmtp = sdp::formats(offer, b.media).into_iter().find(|f| f.pt == b.pt).map(|f| f.fmtp).unwrap_or_default();
    let rate = if b.media == "audio" { 48_000 } else { 90_000 };
    let mut caps = gst::Caps::builder("application/x-rtp")
        .field("media", b.media)
        .field("encoding-name", b.encoding)
        .field("payload", i32::from(b.pt))
        .field("clock-rate", rate);
    // The profile is left to the stream: a payloader held to the offer's
    // profile refuses a programme encoded in another one, and the answer
    // then says what is actually sent.
    let fields = fmtp.split(';').filter_map(|kv| kv.trim().split_once('='));
    for (k, v) in fields.filter(|(k, _)| *k != "profile-level-id") {
        caps = caps.field(k, v);
    }
    transceiver.set_property("codec-preferences", caps.build());
}

/// Answer, set it, and wait for the candidates to be in it.
pub fn answer(bin: &gst::Element) -> Result<String, Refusal> {
    let promise = gst::Promise::new();
    bin.emit_by_name::<()>("create-answer", &[&None::<gst::Structure>, &promise]);
    promise.wait();
    let answer = promise
        .get_reply()
        .and_then(|r| r.get::<gst_webrtc::WebRTCSessionDescription>("answer").ok())
        .ok_or_else(|| {
            Refusal::new(
                406,
                "webrtcbin made no answer to that offer. Offer recvonly video in the codec this output sends, and Opus audio.",
            )
        })?;
    let promise = gst::Promise::new();
    bin.emit_by_name::<()>("set-local-description", &[&answer, &promise]);
    promise.wait();
    let until = Instant::now() + GATHER;
    while Instant::now() < until {
        let state = bin.property::<gst_webrtc::WebRTCICEGatheringState>("ice-gathering-state");
        if state == gst_webrtc::WebRTCICEGatheringState::Complete {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let local = bin.property::<Option<gst_webrtc::WebRTCSessionDescription>>("local-description").unwrap_or(answer);
    local.sdp().as_text().map_err(|e| Refusal::internal(format!("the answer would not print as SDP: {e}")))
}
