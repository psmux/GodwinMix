//! `rist/output`: MPEG-TS over RIST, the contribution protocol broadcasters
//! use between sites (VSF TR-06-1, the Simple Profile).
//!
//! Built like `srt.rs`: a muxer and a sink, and everything else from
//! `output.rs`. RIST is RTP with retransmission requests over RTCP, so the
//! programme is muxed to MPEG-TS, put in RTP, and handed to `ristsink`, which
//! keeps `buffer_ms` of it to answer those requests. It needs no port of its
//! own here: it sends, and the receiver listens.
//!
//! Liveness is the receiver's RTCP reports, and only while they keep coming;
//! see `rist_live`. An output whose receiver has gone quiet is rebuilt by the
//! mixer like any other output that is down (`redial_when_down`), for as long
//! as it takes.

use crate::config::{OutputConfig, Params};
use crate::gstutil::make;
use crate::plugin::output::{link_to_mux, Output, OutputCtx, OutputProvide};
use crate::plugin::source::unknown_method;
use crate::plugin::{
    Capability, CapabilitySet, Configure, Health, Hello, Manifest, MediaDecl, PluginState, ProvideKind, Ready,
    StreamMode, Tier, API_LEVEL,
};
use super::progress::Progress;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use serde_json::{json, Value};

pub const MANIFEST: Manifest = Manifest {
    plugin: "rist",
    id: "output",
    kind: ProvideKind::Output,
    api: API_LEVEL,
    description: "MPEG-TS over RIST (Simple Profile), to a receiver that listens",
    uri_schemes: &["rist://"],
    rank: 240,
    media: MediaDecl { video: StreamMode::Container, audio: StreamMode::Container, alpha: false, thumb: false },
    capabilities: CapabilitySet::new().with(Capability::KeyframeRequest),
    latency_ms: 1000,
    tier: Tier::Core,
};

pub const PROVIDE: OutputProvide = OutputProvide { manifest: MANIFEST, claims, make: new };

fn claims(uri: &str) -> Option<u16> {
    uri.trim().to_lowercase().starts_with("rist://").then_some(MANIFEST.rank)
}

fn new(cfg: &OutputConfig) -> Result<Box<dyn Output>> {
    Ok(Box::new(RistOutput { uri: cfg.uri.clone(), buffer_ms: 1000, sink: Mutex::new(None), heard: Progress::default() }))
}

pub struct RistOutput {
    uri: String,
    buffer_ms: u32,
    sink: Mutex<Option<gst::Element>>,
    /// The receiver's reports, and when they last moved.
    heard: Progress,
}

pub use super::rist_live::address;

impl Output for RistOutput {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        if let Some(u) = hello.params.get("uri").and_then(|v| v.as_str()) {
            self.uri = u.to_string();
        }
        address(&self.uri)?;
        if let Some(v) = hello.params.get("buffer_ms") {
            let n = v.as_integer().filter(|n| (50..=30_000).contains(n));
            self.buffer_ms = n.context("rist/output params.buffer_ms must be 50 to 30000")? as u32;
        }
        let missing = crate::setup::system::absent(&["ristsink", "rtpmp2tpay", "mpegtsmux"]);
        if !missing.is_empty() {
            return Err(crate::setup::system::missing_error("Sending over RIST", crate::setup::system::BAD, &missing));
        }
        Ok(Ready { manifest: MANIFEST, latency_ms: self.buffer_ms, capabilities: MANIFEST.capabilities })
    }

    fn build(&mut self, ctx: &OutputCtx<'_>, video: &gst::Element, audio: &gst::Element) -> Result<()> {
        let (id, gen) = (ctx.id, ctx.generation);
        let (host, port) = address(&self.uri)?;
        let mux = make("mpegtsmux", &format!("out-{id}-mux-{gen}"))?;
        crate::probe::set_int(&mux, "si-interval", 9_000);
        // Seven packets a datagram, as every RIST and SMPTE 2022 receiver expects.
        crate::probe::set_int(&mux, "alignment", 7);
        let pay = make("rtpmp2tpay", &format!("out-{id}-pay-{gen}"))?;
        let sink = make("ristsink", &format!("out-{id}-rist-{gen}"))?;
        sink.set_property("address", &host);
        sink.set_property("port", u32::from(port));
        crate::probe::set_int(&sink, "sender-buffer", i64::from(self.buffer_ms));
        ctx.pipeline.add_many([&mux, &pay, &sink]).context("adding the rist muxer and sink")?;
        super::ts::link_video(ctx, video, &mux)?;
        link_to_mux(audio, &mux, super::ts::TS_PADS)?;
        gst::Element::link_many([&mux, &pay, &sink]).context("linking muxer to rist sink")?;
        *self.sink.lock() = Some(sink);
        self.heard.reset();
        Ok(())
    }

    fn connected(&self) -> bool {
        let held = self.sink.lock();
        let Some(sink) = held.as_ref() else { return false };
        let n = super::rist_live::answered(sink).unwrap_or_else(|| super::rist_live::round_trip(sink));
        self.heard.live(n, std::time::Instant::now())
    }

    /// The receiver listens and this sends, so a receiver gone quiet is
    /// asked again every `DOWN_FOR` until it answers.
    fn redial_when_down(&self) -> bool {
        true
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        if let Some(u) = params.get("uri").and_then(|v| v.as_str()) {
            address(u)?;
        }
        Ok(Configure::RestartRequired("a rist output takes a new address by reconnecting".into()))
    }

    fn health(&self) -> Health {
        Health::of(if self.connected() { PluginState::Running } else { PluginState::Starting })
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value> {
        match method {
            "stats" => {
                let held = self.sink.lock();
                let stats = held.as_ref().and_then(|s| s.property::<Option<gst::Structure>>("stats"));
                Ok(json!({ "stats": stats.map(|s| s.to_string()).unwrap_or_default() }))
            }
            other => Err(unknown_method(&MANIFEST, other, &["stats"])),
        }
    }
}

#[cfg(test)]
#[path = "rist_tests.rs"]
mod tests;
