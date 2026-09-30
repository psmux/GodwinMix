//! One WHEP viewer: a `webrtcbin` on the output's tees, the offer in and the
//! answer out.
//!
//! ```text
//!   video tee ──► queue (leaky) ──► rtp payloader ──┐
//!                                                   ├──► webrtcbin ──► the viewer
//!   Opus tee  ──► queue (leaky) ──► rtpopuspay ─────┘
//! ```
//!
//! Nothing here encodes. The payloader is set to the payload type the
//! viewer's offer gave the codec, and the transceiver's codec preference is
//! the offer's own line, so the answer can only say yes to what is sent.
//! A viewer that stops reading fills its own queue, which drops, and holds
//! nobody else up.

use super::negotiate::{answer, mline, prefer, webrtcbin, Branch};
use super::params::{VideoSend, WhepParams};
use super::sdp;
use super::Refusal;
use crate::gstutil::{self, make};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_sdp as gst_sdp;
use gstreamer_webrtc as gst_webrtc;

/// What a session hangs off.
pub struct Tees<'a> {
    pub pipeline: &'a gst::Pipeline,
    pub video: &'a gst::Element,
    pub audio: Option<&'a gst::Element>,
    pub send: VideoSend,
}

pub struct Session {
    pub bin: gst::Element,
    pipeline: gst::Pipeline,
    elements: Vec<gst::Element>,
    branches: Vec<(gst::Element, gst::Pad)>,
}

impl Session {
    /// Answer `offer`, named `name` (unique in the pipeline).
    pub fn start(tees: &Tees<'_>, offer: &str, params: &WhepParams, name: &str) -> Result<(Session, String), Refusal> {
        let message = gst_sdp::SDPMessage::parse_buffer(offer.as_bytes()).map_err(|_| {
            Refusal::bad("the body is not SDP. POST the RTCPeerConnection's offer, with Content-Type: application/sdp.")
        })?;
        let profile = tees
            .video
            .static_pad("sink")
            .and_then(|p| p.current_caps())
            .and_then(|c| c.structure(0).and_then(|s| s.get::<String>("profile").ok()));
        let video_pt = sdp::pick(offer, "video", tees.send.encoding, profile.as_deref()).ok_or_else(|| {
            Refusal::new(406, format!(
                "the offer has no {} video. This output sends {}; offer it (for a browser, add a recvonly video transceiver).",
                tees.send.encoding, tees.send.encoding
            ))
        })?;
        let audio_pt = tees.audio.and_then(|_| sdp::pick(offer, "audio", "OPUS", None));
        let bin = webrtcbin(name, params)?;
        let mut session = Session { bin: bin.clone(), pipeline: tees.pipeline.clone(), elements: vec![bin.clone()], branches: Vec::new() };
        tees.pipeline.add(&bin).map_err(|e| Refusal::internal(format!("could not add the viewer's webrtcbin: {e}")))?;
        bin.sync_state_with_parent().ok();
        let result = session.negotiate(tees, message, offer, (video_pt, audio_pt), name);
        match result {
            Ok(answer) => Ok((session, answer)),
            Err(e) => {
                session.end();
                Err(e)
            }
        }
    }

    fn negotiate(&mut self, tees: &Tees<'_>, message: gst_sdp::SDPMessage, offer: &str, pts: (u8, Option<u8>), name: &str) -> Result<String, Refusal> {
        let remote = gst_webrtc::WebRTCSessionDescription::new(gst_webrtc::WebRTCSDPType::Offer, message);
        let promise = gst::Promise::new();
        self.bin.emit_by_name::<()>("set-remote-description", &[&remote, &promise]);
        promise.wait();
        let video = Branch { tee: tees.video, media: "video", payloader: tees.send.payloader, encoding: tees.send.encoding, pt: pts.0 };
        self.branch(&video, offer, name)?;
        if let (Some(tee), Some(pt)) = (tees.audio, pts.1) {
            self.branch(&Branch { tee, media: "audio", payloader: "rtpopuspay", encoding: "OPUS", pt }, offer, name)?;
        }
        answer(&self.bin)
    }

    /// One tee to the webrtcbin pad for the offer's section of `media`.
    fn branch(&mut self, b: &Branch<'_>, offer: &str, name: &str) -> Result<(), Refusal> {
        let internal = |e: anyhow::Error| Refusal::internal(format!("{e:#}"));
        let q = gstutil::queue_time(&format!("{name}-{}q", b.media), 1.0, true).map_err(internal)?;
        let pay = make(b.payloader, &format!("{name}-{}pay", b.media)).map_err(internal)?;
        pay.set_property("pt", u32::from(b.pt));
        crate::probe::set_int(&pay, "config-interval", -1);
        crate::probe::set_enum(&pay, "aggregate-mode", "zero-latency");
        self.pipeline.add_many([&q, &pay]).map_err(|e| Refusal::internal(e.to_string()))?;
        self.elements.extend([q.clone(), pay.clone()]);
        q.link(&pay).map_err(|e| Refusal::internal(e.to_string()))?;
        let index = mline(offer, b.media).ok_or_else(|| Refusal::bad(format!("the offer has no {} section", b.media)))?;
        prefer(&self.bin, index, offer, b);
        let sink = self
            .bin
            .request_pad_simple(&format!("sink_{index}"))
            .ok_or_else(|| Refusal::internal(format!("webrtcbin gave no pad for the {} section", b.media)))?;
        pay.static_pad("src")
            .and_then(|p| p.link(&sink).ok())
            .ok_or_else(|| Refusal::internal(format!("could not link the {} payloader to webrtcbin", b.media)))?;
        for e in [&pay, &q] {
            e.sync_state_with_parent().ok();
        }
        let pad = b.tee.request_pad_simple("src_%u").ok_or_else(|| Refusal::internal("the output's tee refused a pad"))?;
        pad.link(&q.static_pad("sink").expect("a queue has a sink pad"))
            .map_err(|e| Refusal::internal(format!("could not link the viewer to the {} tee: {e}", b.media)))?;
        self.branches.push((b.tee.clone(), pad));
        if b.media == "video" {
            if let Some(src) = q.static_pad("src") {
                gstutil::force_keyframe(&src);
            }
        }
        Ok(())
    }

    /// The peer connection's state, for the watcher.
    pub fn state(&self) -> gst_webrtc::WebRTCPeerConnectionState {
        self.bin.property("connection-state")
    }

    /// Unlink from the tees and take every element out. Never called on a
    /// streaming thread.
    pub fn end(&mut self) {
        for (tee, pad) in self.branches.drain(..) {
            if let Some(peer) = pad.peer() {
                let _ = pad.unlink(&peer);
            }
            tee.release_request_pad(&pad);
        }
        for el in self.elements.drain(..) {
            el.set_locked_state(true);
            let _ = el.set_state(gst::State::Null);
            let _ = self.pipeline.remove(&el);
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.end();
    }
}

