//! `srt/output`: MPEG-TS over SRT.
//!
//! This file is the proof that the seam works. Everything an output shares
//! with every other output (the feed queues that ride out an outage, the
//! proxy pair, the reconnect backoff, the overflow watchdog, the keyframe
//! request) is in `output.rs` and none of it is repeated here. What is here is
//! a muxer, a sink, and an honest answer to "are we connected".
//!
//! Liveness is read from `srtsink`'s own statistics. The field names differ
//! between caller and listener mode and between GStreamer versions, so several
//! are tried and the first one present wins; a build whose `srtsink` reports no
//! statistics at all falls back to the element's state, which is the honest
//! answer for a sink that cannot say more.

use crate::config::{OutputConfig, Params};
use crate::gstutil::make;
use crate::plugin::output::{link_to_mux, Output, OutputCtx, OutputProvide};
use crate::plugin::source::unknown_method;
use crate::plugin::{
    Capability, CapabilitySet, Configure, Health, Hello, Manifest, MediaDecl, PluginState,
    ProvideKind, Ready, StreamMode, Tier, API_LEVEL,
};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use serde_json::{json, Value};

pub const MANIFEST: Manifest = Manifest {
    plugin: "srt",
    id: "output",
    kind: ProvideKind::Output,
    api: API_LEVEL,
    description: "MPEG-TS over SRT, in caller or listener mode",
    uri_schemes: &["srt://"],
    rank: 240,
    media: MediaDecl {
        video: StreamMode::Container,
        audio: StreamMode::Container,
        alpha: false,
        thumb: false,
    },
    capabilities: CapabilitySet::new().with(Capability::KeyframeRequest),
    // The default SRT receive buffer. Declared so a client asking what the
    // chain costs gets a number rather than a shrug.
    latency_ms: 125,
    tier: Tier::Core,
};

pub const PROVIDE: OutputProvide = OutputProvide { manifest: MANIFEST, claims, make: new };

/// Fields on `srtsink`'s stats structure that mean "somebody is there", tried
/// in order.
const LIVE_FIELDS: &[&str] = &["packets-sent", "bytes-sent", "bytes-sent-total", "packets-sent-total"];

fn claims(uri: &str) -> Option<u16> {
    uri.trim().to_lowercase().starts_with("srt://").then_some(MANIFEST.rank)
}

fn new(cfg: &OutputConfig) -> Result<Box<dyn Output>> {
    Ok(Box::new(SrtOutput { uri: cfg.uri.clone(), latency_ms: None, sink: Mutex::new(None) }))
}

pub struct SrtOutput {
    uri: String,
    latency_ms: Option<i64>,
    sink: Mutex<Option<gst::Element>>,
}

impl Output for SrtOutput {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        validate(&hello.params)?;
        anyhow::ensure!(
            crate::probe::exists("srtsink") && crate::probe::exists("mpegtsmux"),
            "sending over SRT needs the GStreamer `srtsink` and `mpegtsmux` elements \
             (gstreamer1.0-plugins-bad on Debian and Ubuntu, gst-plugins-bad elsewhere)"
        );
        if let Some(u) = hello.params.get("uri").and_then(|v| v.as_str()) {
            self.uri = u.to_string();
        }
        self.latency_ms = hello.params.get("latency_ms").and_then(|v| v.as_integer());
        anyhow::ensure!(!self.uri.trim().is_empty(), "srt/output needs an address in params.uri");
        let latency_ms = self.latency_ms.unwrap_or(MANIFEST.latency_ms as i64).max(0) as u32;
        Ok(Ready { manifest: MANIFEST, latency_ms, capabilities: MANIFEST.capabilities })
    }

    fn build(
        &mut self,
        ctx: &OutputCtx<'_>,
        video: &gst::Element,
        audio: &gst::Element,
    ) -> Result<()> {
        let (id, gen) = (ctx.id, ctx.generation);
        let mux = make("mpegtsmux", &format!("out-{id}-mux-{gen}"))?;
        // A PAT and PMT every 100 ms, so a receiver that joins mid stream can
        // start without waiting for the next scheduled table. The programme's
        // own keyframe interval is what decides when it can decode; this only
        // stops the tables being the thing it waits for.
        crate::probe::set_int(&mux, "si-interval", 9_000);
        crate::probe::set_bool(&mux, "alignment", false);

        let sink = make("srtsink", &format!("out-{id}-srt-{gen}"))?;
        sink.set_property("uri", &self.uri);
        if let Some(ms) = self.latency_ms {
            crate::probe::set_int(&sink, "latency", ms.max(0));
        }
        // Do not block the muxer when a receiver goes away. The buffer that
        // rides out an outage is the feed queue on the programme side, which
        // is where it has to be: it survives the pipeline being rebuilt.
        crate::probe::set_bool(&sink, "wait-for-connection", false);
        crate::probe::set_bool(&sink, "sync", false);
        crate::probe::set_bool(&sink, "async", false);

        ctx.pipeline.add_many([&mux, &sink]).context("adding the srt muxer and sink")?;
        // `mpegtsmux` names both its request pads `sink_%d`; older builds spell
        // it `sink_%u`. Both are asked for.
        link_to_mux(video, &mux, &["sink_%d", "sink_%u"])?;
        link_to_mux(audio, &mux, &["sink_%d", "sink_%u"])?;
        mux.link(&sink).context("linking muxer to srt sink")?;
        *self.sink.lock() = Some(sink);
        Ok(())
    }

    fn connected(&self) -> bool {
        let held = self.sink.lock();
        let Some(sink) = held.as_ref() else { return false };
        let stats = sink.property::<Option<gst::Structure>>("stats");
        if let Some(s) = stats {
            // Caller mode puts the numbers at the top level; listener mode puts
            // one structure per caller in `callers`.
            if let Some(n) = first_number(&s) {
                return n > 0;
            }
            if let Ok(callers) = s.get::<gst::List>("callers") {
                return callers.iter().any(|v| {
                    v.get::<gst::Structure>().ok().and_then(|c| first_number(&c)).unwrap_or(0) > 0
                });
            }
        }
        // No statistics on this build: the state is the most this sink can
        // honestly say.
        sink.current_state() == gst::State::Playing
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        validate(params)?;
        Ok(Configure::RestartRequired("an srt output takes a new address by reconnecting".into()))
    }

    fn health(&self) -> Health {
        Health::of(if self.connected() { PluginState::Running } else { PluginState::Starting })
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value> {
        match method {
            "stats" => {
                let stats = self
                    .sink
                    .lock()
                    .as_ref()
                    .and_then(|s| s.property::<Option<gst::Structure>>("stats"))
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                Ok(json!({ "stats": stats }))
            }
            other => Err(unknown_method(&MANIFEST, other, &["stats"])),
        }
    }
}

/// The first of `LIVE_FIELDS` this structure carries, whatever integer width
/// the version chose for it.
fn first_number(s: &gst::StructureRef) -> Option<i64> {
    for field in LIVE_FIELDS.iter().copied() {
        if let Ok(v) = s.get::<i64>(field) {
            return Some(v);
        }
        if let Ok(v) = s.get::<u64>(field) {
            return Some(v as i64);
        }
        if let Ok(v) = s.get::<i32>(field) {
            return Some(v as i64);
        }
    }
    None
}

pub fn validate(params: &Params) -> Result<()> {
    for (key, value) in params {
        match key.as_str() {
            "uri" => {
                let s = value.as_str().unwrap_or_default();
                anyhow::ensure!(
                    s.to_lowercase().starts_with("srt://"),
                    "srt/output params.uri must be an srt:// address, not `{s}`"
                );
            }
            "latency_ms" => {
                let n = value.as_integer().unwrap_or(-1);
                anyhow::ensure!(
                    (0..=10_000).contains(&n),
                    "srt/output params.latency_ms must be 0 to 10000, not `{value}`"
                );
            }
            _ => {}
        }
    }
    Ok(())
}
