//! RIST Simple Profile, listening on an even port.
//!
//! ```text
//!   ristsrc ──► rtpmp2tdepay ──(probe)──► parsebin ──► tags
//! ```

use std::sync::Arc;

use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::json;

use super::loss::Loss;
use super::pads::{make, parse_into, Pads};
use super::runner::Plan;
use super::spec::{InputError, InputSpec};

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
