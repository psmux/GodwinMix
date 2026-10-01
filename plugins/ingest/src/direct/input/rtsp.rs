//! RTSP from a camera or an encoder, over TCP or UDP.
//!
//! ```text
//!   rtspsrc ──(each RTP stream)──► parsebin (depayloads) ──► tags
//! ```
//!
//! Loss is what the RTP sessions say the camera sent and never came.

use std::sync::Arc;

use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::json;

use super::loss::Loss;
use super::pads::{make, parse_into, Pads};
use super::pull::redact;
use super::runner::Plan;
use super::spec::{InputError, InputSpec};

pub struct Rtsp {
    uri: String,
    /// `tcp`, `udp`, or empty to let rtspsrc try UDP first.
    transport: String,
    latency_ms: u32,
}

impl Rtsp {
    pub fn new(spec: &InputSpec) -> Result<Rtsp, InputError> {
        let transport = spec.param("transport").unwrap_or_default().to_ascii_lowercase();
        if !["", "tcp", "udp", "auto"].contains(&transport.as_str()) {
            return Err(InputError::new(
                format!("RTSP transport '{transport}' is not one of tcp, udp or auto. Choose tcp for a camera across a firewall or NAT."),
                json!({"field": "params.transport", "got": transport, "allowed": ["tcp", "udp", "auto"]}),
            ));
        }
        let latency_ms = spec.number("latency_ms").unwrap_or(200).min(10_000) as u32;
        Ok(Rtsp { uri: spec.uri.clone(), transport, latency_ms })
    }
}

impl Plan for Rtsp {
    fn build(&mut self, pipeline: &gst::Pipeline, pads: &Arc<Pads>) -> Result<Loss, String> {
        let src = make("rtspsrc")?;
        src.set_property("location", &self.uri);
        src.set_property("latency", self.latency_ms);
        match self.transport.as_str() {
            "tcp" => src.set_property_from_str("protocols", "tcp"),
            "udp" => src.set_property_from_str("protocols", "udp"),
            _ => {}
        }
        let manager = Arc::new(std::sync::Mutex::new(None::<gst::Element>));
        let keep = manager.clone();
        src.connect("new-manager", false, move |args| {
            *keep.lock().unwrap_or_else(|e| e.into_inner()) = args.get(1).and_then(|v| v.get::<gst::Element>().ok());
            None
        });
        let (weak, shared) = (pipeline.downgrade(), pads.clone());
        src.connect_pad_added(move |_, pad| {
            if let Some(pipeline) = weak.upgrade() {
                if let Err(e) = parse_into(&pipeline, pad, &shared) {
                    shared.note.lock().unwrap_or_else(|e| e.into_inner()).push(e);
                }
            }
        });
        pipeline.add(&src).map_err(|e| e.to_string())?;
        Ok(Loss::none().with_poll(move || manager.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map_or(0, rtp_lost)))
    }

    fn address(&self) -> String {
        redact(&self.uri)
    }
}

/// RTP packets the camera sent that never came, from each session's
/// statistics of the sources it heard from.
fn rtp_lost(rtpbin: &gst::Element) -> u64 {
    (0u32..4)
        .filter_map(|i| rtpbin.emit_by_name::<Option<glib::Object>>("get-session", &[&i]))
        .map(|session| {
            let stats = session.property::<gst::Structure>("stats");
            let sources = stats.get::<glib::ValueArray>("source-stats").ok();
            sources
                .iter()
                .flat_map(|a| a.iter())
                .filter_map(|v| v.get::<gst::Structure>().ok())
                .filter(|s| !s.get::<bool>("internal").unwrap_or(true))
                .filter_map(|s| s.get::<i32>("packets-lost").ok())
                .map(|n| n.max(0) as u64)
                .sum::<u64>()
        })
        .sum()
}
