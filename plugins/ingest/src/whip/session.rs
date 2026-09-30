//! One WHIP publisher: a `webrtcbin`, the offer in, the answer out, and its
//! media on the way to the hub.
//!
//! ```text
//!   webrtcbin ─┬─► rtph264depay ──► h264parse ──► appsink (video tags)
//!              └─► rtpopusdepay ──► opusdec ──► avenc_aac ──► aacparse ──► appsink (audio tags)
//! ```
//!
//! The video is never decoded: the answer offers H.264 only, so a browser
//! sends H.264 and its access units go to the hub as they came. The sound is
//! the one thing transcoded anywhere in this plugin, Opus to AAC, because
//! WebRTC carries Opus and the hub carries what FLV can. An audio transcode
//! is a few percent of one core; a video one would not be, which is why
//! VP8 and VP9 are not accepted.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gmx_netkit::pipe::Pipe;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_sdp as gst_sdp;
use gstreamer_webrtc as gst_webrtc;

use crate::channels::{Admit, Protocol};
use crate::gate::ChannelGate;
use crate::rtmp::Kick;
use crate::tagger::{self, Shared, Zero};

mod branch;

/// How long an answer waits for ICE to gather its candidates. Host
/// candidates take milliseconds; this is for a slow interface.
const GATHER: Duration = Duration::from_secs(3);

pub struct Session {
    pipe: Option<Pipe>,
    to: Shared,
    stop: Arc<AtomicBool>,
}

impl Session {
    /// Answer `offer` and start taking media. `ended` is called, from a
    /// thread of the session's own, when the peer goes away without saying.
    pub fn start(
        gate: &Arc<ChannelGate>,
        admit: Admit,
        offer: &str,
        peer: &str,
        ports: (u16, u16),
        ended: Box<dyn Fn() + Send>,
    ) -> Result<(Session, String), String> {
        gmx_netkit::init()?;
        let sdp = gst_sdp::SDPMessage::parse_buffer(offer.as_bytes()).map_err(|_| "the offer is not SDP. Send the RTCPeerConnection's offer as the request body, with Content-Type application/sdp.".to_string())?;
        if !offer.contains("H264") {
            return Err("the offer has no H.264 video. Channels take H.264 from WebRTC so the picture is never decoded; set the browser or encoder to H.264.".into());
        }
        let stop = Arc::new(AtomicBool::new(false));
        let halt = stop.clone();
        let kick: Kick = Arc::new(move || halt.store(true, Ordering::Relaxed));
        let name = format!("{}-{}", admit.app, admit.stream);
        let inlet = gate.let_in(Protocol::Whip, admit, peer, kick)?;
        let to = tagger::share(inlet);
        let (pipe, bin) = build(&name, ports, &to)?;
        let session = Session { pipe: Some(pipe), to, stop: stop.clone() };
        let answer = negotiate(&bin, sdp)?;
        watch(bin, stop, ended);
        Ok((session, answer))
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        drop(self.pipe.take());
        tagger::close(&self.to);
    }
}

fn build(name: &str, ports: (u16, u16), to: &Shared) -> Result<(Pipe, gst::Element), String> {
    let pipeline = gst::Pipeline::with_name(&format!("whip-{name}"));
    let bin = gst::ElementFactory::make("webrtcbin")
        .property_from_str("bundle-policy", "max-bundle")
        .property("latency", 200u32)
        .build()
        .map_err(|e| format!("GStreamer has no webrtcbin, so WHIP cannot be taken: {e}"))?;
    let ice = bin.property::<Option<glib::Object>>("ice-agent");
    if let Some(ice) = ice.filter(|i| i.has_property("min-rtp-port")) {
        ice.set_property("min-rtp-port", u32::from(ports.0));
        ice.set_property("max-rtp-port", u32::from(ports.1));
    }
    pipeline.add(&bin).map_err(|e| e.to_string())?;
    for (kind, caps) in [("video", "application/x-rtp,media=video,encoding-name=H264,clock-rate=90000"), ("audio", "application/x-rtp,media=audio,encoding-name=OPUS,clock-rate=48000")] {
        let caps: gst::Caps = caps.parse().map_err(|_| format!("the {kind} caps did not parse"))?;
        let direction = gst_webrtc::WebRTCRTPTransceiverDirection::Recvonly;
        let _ = bin.emit_by_name::<Option<gst_webrtc::WebRTCRTPTransceiver>>("add-transceiver", &[&direction, &caps]);
    }
    let zero = Arc::new(Zero::default());
    let (weak, shared) = (pipeline.downgrade(), to.clone());
    bin.connect_pad_added(move |_, pad| {
        if let Some(pipeline) = weak.upgrade() {
            branch::attach(&pipeline, pad, &shared, &zero);
        }
    });
    let mut pipe = Pipe::wrap(pipeline);
    pipe.play(None)?;
    Ok((pipe, bin))
}

/// Offer in, answer out, once ICE has its candidates.
fn negotiate(bin: &gst::Element, sdp: gst_sdp::SDPMessage) -> Result<String, String> {
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
        .ok_or("webrtcbin made no answer to that offer. It may offer nothing this can receive: H.264 video and Opus audio.")?;
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
    local.sdp().as_text().map_err(|e| format!("the answer would not print as SDP: {e}"))
}

/// A thread that watches the connection, once a second, and ends the session
/// when the peer has gone or the gate has cut it off.
fn watch(bin: gst::Element, stop: Arc<AtomicBool>, ended: Box<dyn Fn() + Send>) {
    let _ = std::thread::Builder::new().name("gmx-whip-watch".into()).spawn(move || {
        use gst_webrtc::WebRTCPeerConnectionState as State;
        let started = Instant::now();
        loop {
            std::thread::sleep(Duration::from_secs(1));
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let state = bin.property::<State>("connection-state");
            let never = state != State::Connected && started.elapsed() > Duration::from_secs(20);
            if matches!(state, State::Failed | State::Closed | State::Disconnected) || never {
                break;
            }
        }
        drop(bin);
        ended();
    });
}
