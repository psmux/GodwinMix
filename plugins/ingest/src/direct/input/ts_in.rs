//! The MPEG-TS transports: UDP and RTP (unicast or multicast, source
//! specific where the OS allows), SRT (caller or listener) and RIST.
//!
//! ```text
//!   udpsrc                       ──(probe)──► parsebin ──► tags
//!   srtsrc                       ──(probe)──► parsebin ──► tags
//!   ristsrc ──► rtpmp2tdepay     ──(probe)──► parsebin ──► tags
//! ```
//!
//! The probe is the udp plugin's: it reads the PAT, PMT and SDT, keeps the
//! chosen program, drops stuffing, and counts continuity errors and RTP gaps.
//! It never waits, so a burst of loss costs counted packets and nothing more.

use std::sync::Arc;

use gmx_udp::recv::settings::Settings;
use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::{json, Value};

use super::loss::Loss;
use super::pads::{make, parse_into, Pads};
use super::runner::Plan;
use super::spec::{InputError, InputSpec};

pub struct Udp {
    settings: Settings,
    address: String,
    program: Option<u16>,
}

impl Udp {
    pub fn new(spec: &InputSpec) -> Result<Udp, InputError> {
        let mut params = spec.params.clone();
        params["uri"] = json!(spec.uri);
        let settings = Settings::from_params(&params).map_err(|e| InputError::new(e, json!({"field": "uri", "got": spec.uri})))?;
        let address = settings.endpoint().map(|e| e.display(settings.scheme())).unwrap_or_else(|_| spec.uri.clone());
        Ok(Udp { settings, address, program: spec.program })
    }
}

impl Plan for Udp {
    fn build(&mut self, pipeline: &gst::Pipeline, pads: &Arc<Pads>) -> Result<Loss, String> {
        let src = gmx_udp::recv::elements::udpsrc(&self.settings)?;
        pipeline.add(&src).map_err(|e| e.to_string())?;
        let loss = Loss::probe(&src, self.program)?;
        parse_into(pipeline, &src.static_pad("src").ok_or("udpsrc has no src pad")?, pads)?;
        Ok(loss)
    }

    fn address(&self) -> String {
        self.address.clone()
    }

    fn program(&self) -> Option<u16> {
        self.program
    }

    /// The socket stays open and joined whether anyone sends or not, so a
    /// quiet feed is waited for, never reconnected.
    fn stall_ms(&self) -> Option<u64> {
        None
    }
}

pub struct Srt {
    uri: String,
    params: Value,
    program: Option<u16>,
}

impl Srt {
    pub fn new(spec: &InputSpec) -> Srt {
        // `srt://@:9000` is how VLC and ffmpeg users write a listener.
        let mut uri = spec.uri.replacen("srt://@", "srt://", 1);
        let host = uri.trim_start_matches("srt://").split([':', '?', '/']).next().unwrap_or("");
        let listening = host.is_empty() || host == "0.0.0.0" || spec.param("mode").as_deref() == Some("listener");
        if listening && !uri.contains("mode=") {
            uri.push_str(if uri.contains('?') { "&mode=listener" } else { "?mode=listener" });
        }
        Srt { uri, params: spec.params.clone(), program: spec.program }
    }
}

impl Plan for Srt {
    fn build(&mut self, pipeline: &gst::Pipeline, pads: &Arc<Pads>) -> Result<Loss, String> {
        let src = make("srtsrc")?;
        src.set_property("uri", &self.uri);
        let text = |k: &str| self.params.get(k).and_then(Value::as_str).filter(|s| !s.is_empty());
        if let Some(p) = text("passphrase") {
            src.set_property("passphrase", p);
        }
        if let Some(id) = text("streamid") {
            src.set_property("streamid", id);
        }
        if let Some(ms) = self.params.get("latency_ms").and_then(Value::as_u64) {
            src.set_property("latency", ms.min(60_000) as i32);
        }
        pipeline.add(&src).map_err(|e| e.to_string())?;
        let stats = src.clone();
        let loss = Loss::probe(&src, self.program)?.with_poll(move || srt_lost(&stats));
        parse_into(pipeline, &src.static_pad("src").ok_or("srtsrc has no src pad")?, pads)?;
        Ok(loss)
    }

    fn address(&self) -> String {
        self.uri.clone()
    }

    fn program(&self) -> Option<u16> {
        self.program
    }

    /// A listener waits for its caller as long as it takes.
    fn stall_ms(&self) -> Option<u64> {
        (!self.uri.contains("mode=listener")).then_some(10_000)
    }
}

/// Packets SRT gave up on, from the caller's statistics or each caller's.
fn srt_lost(src: &gst::Element) -> u64 {
    let stats = src.property::<gst::Structure>("stats");
    let one = |s: &gst::StructureRef| -> u64 {
        ["packets-received-lost", "pkt-rcv-loss"]
            .iter()
            .find_map(|k| s.get::<i32>(*k).map(|v| v.max(0) as u64).or_else(|_| s.get::<i64>(*k).map(|v| v.max(0) as u64)).ok())
            .unwrap_or(0)
    };
    let callers = stats.get::<glib::ValueArray>("callers").ok();
    let from_callers: u64 = callers.iter().flat_map(|a| a.iter()).filter_map(|v| v.get::<gst::Structure>().ok()).map(|s| one(&s)).sum();
    one(&stats) + from_callers
}

pub struct Rist {
    address: String,
    port: u16,
    program: Option<u16>,
    uri: String,
}

impl Rist {
    pub fn new(spec: &InputSpec) -> Result<Rist, InputError> {
        let rest = spec.uri.split_once("://").map_or("", |(_, r)| r).trim_start_matches('@');
        let hostport = rest.split(['?', '/']).next().unwrap_or("");
        let (host, port) = hostport.rsplit_once(':').unwrap_or((hostport, ""));
        let port = port.parse::<u16>().ok().filter(|p| p % 2 == 0).ok_or_else(|| {
            InputError::new(
                format!("'{}' needs an even port: RIST sends RTP on it and RTCP on the next one up. Write rist://@0.0.0.0:5004.", spec.uri),
                json!({"field": "uri", "got": spec.uri}),
            )
        })?;
        let address = if host.is_empty() { "0.0.0.0".to_string() } else { host.trim_matches(['[', ']']).to_string() };
        Ok(Rist { address, port, program: spec.program, uri: spec.uri.clone() })
    }
}

impl Plan for Rist {
    fn build(&mut self, pipeline: &gst::Pipeline, pads: &Arc<Pads>) -> Result<Loss, String> {
        let src = make("ristsrc")?;
        src.set_property("address", &self.address);
        src.set_property("port", u32::from(self.port));
        let caps = gst::Caps::builder("application/x-rtp").field("media", "video").field("clock-rate", 90_000i32).field("encoding-name", "MP2T").build();
        src.set_property("caps", &caps);
        let depay = make("rtpmp2tdepay")?;
        pipeline.add_many([&src, &depay]).map_err(|e| e.to_string())?;
        src.link(&depay).map_err(|e| e.to_string())?;
        let stats = src.clone();
        let loss = Loss::probe(&depay, self.program)?.with_poll(move || rist_lost(&stats));
        parse_into(pipeline, &depay.static_pad("src").ok_or("rtpmp2tdepay has no src pad")?, pads)?;
        Ok(loss)
    }

    fn address(&self) -> String {
        self.uri.clone()
    }

    fn program(&self) -> Option<u16> {
        self.program
    }

    fn stall_ms(&self) -> Option<u64> {
        None
    }
}

/// Packets RIST could not recover by retransmission.
fn rist_lost(src: &gst::Element) -> u64 {
    let stats = src.property::<gst::Structure>("stats");
    let sessions = stats.get::<glib::ValueArray>("session-stats").ok();
    sessions
        .iter()
        .flat_map(|a| a.iter())
        .filter_map(|v| v.get::<gst::Structure>().ok())
        .filter_map(|s| s.get::<u64>("permanently-lost").ok())
        .sum::<u64>()
        + stats.get::<u64>("permanently-lost").unwrap_or(0)
}
