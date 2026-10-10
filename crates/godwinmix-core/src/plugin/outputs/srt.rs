//! `srt/output`: MPEG-TS over SRT.
//!
//! This file is the proof that the seam works. Everything an output shares
//! with every other output (the feed queues that ride out an outage, the
//! proxy pair, the reconnect backoff, the overflow watchdog, the keyframe
//! request) is in `output.rs` and none of it is repeated here. What is here is
//! a muxer, a sink, and an honest answer to "are we connected".
//!
//! Liveness is the receiver's acknowledgements, read from `srtsink`'s own
//! statistics and judged on whether they are still coming; see `srt_live` and
//! `progress`. A caller that has heard nothing back for a while is rebuilt by
//! the mixer like any other output that is down (`redial_when_down`); a
//! listener waits for its callers and is left alone.

use crate::config::{OutputConfig, Params};
use crate::gstutil::make;
use crate::plugin::output::{link_to_mux, Output, OutputCtx, OutputProvide};
use crate::plugin::source::unknown_method;
use crate::plugin::{Capability, CapabilitySet, Configure, Health, Hello, Manifest, MediaDecl, PluginState};
use crate::plugin::{ProvideKind, Ready, StreamMode, Tier, API_LEVEL};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use serde_json::{json, Value};
use super::progress::Progress;

pub const MANIFEST: Manifest = Manifest {
    plugin: "srt",
    id: "output",
    kind: ProvideKind::Output,
    api: API_LEVEL,
    description: "MPEG-TS over SRT, in caller or listener mode",
    uri_schemes: &["srt://"],
    rank: 240,
    media: MediaDecl { video: StreamMode::Container, audio: StreamMode::Container, alpha: false, thumb: false },
    capabilities: CapabilitySet::new().with(Capability::KeyframeRequest),
    // The default SRT receive buffer. Declared so a client asking what the
    // chain costs gets a number rather than a shrug.
    latency_ms: 125,
    tier: Tier::Core,
};

pub const PROVIDE: OutputProvide = OutputProvide { manifest: MANIFEST, claims, make: new };

fn claims(uri: &str) -> Option<u16> {
    uri.trim().to_lowercase().starts_with("srt://").then_some(MANIFEST.rank)
}

fn new(cfg: &OutputConfig) -> Result<Box<dyn Output>> {
    Ok(Box::new(SrtOutput { uri: cfg.uri.clone(), latency_ms: None, sink: Mutex::new(None), heard: Progress::default() }))
}

pub struct SrtOutput {
    uri: String,
    latency_ms: Option<i64>,
    sink: Mutex<Option<gst::Element>>,
    /// The receiver's acknowledgements, and when they last moved.
    heard: Progress,
}

impl Output for SrtOutput {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        validate(&hello.params)?;
        let missing = crate::setup::system::absent(&["srtsink", "mpegtsmux"]);
        if !missing.is_empty() {
            return Err(crate::setup::system::missing_error("Sending over SRT", crate::setup::system::BAD, &missing));
        }
        if let Some(u) = hello.params.get("uri").and_then(|v| v.as_str()) {
            self.uri = u.to_string();
        }
        self.latency_ms = hello.params.get("latency_ms").and_then(|v| v.as_integer());
        anyhow::ensure!(!self.uri.trim().is_empty(), "srt/output needs an address in params.uri");
        let latency_ms = self.latency_ms.unwrap_or(MANIFEST.latency_ms as i64).max(0) as u32;
        Ok(Ready { manifest: MANIFEST, latency_ms, capabilities: MANIFEST.capabilities })
    }

    fn build(&mut self, ctx: &OutputCtx<'_>, video: &gst::Element, audio: &gst::Element) -> Result<()> {
        let (id, gen) = (ctx.id, ctx.generation);
        let mux = make("mpegtsmux", &format!("out-{id}-mux-{gen}"))?;
        // A PAT and PMT every 100 ms, so a receiver joining mid stream waits
        // for a keyframe and never for the tables.
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
        // The picture goes through a parser of its own; see `ts`.
        super::ts::link_video(ctx, video, &mux)?;
        link_to_mux(audio, &mux, super::ts::TS_PADS)?;
        mux.link(&sink).context("linking muxer to srt sink")?;
        *self.sink.lock() = Some(sink);
        self.heard.reset();
        Ok(())
    }

    fn connected(&self) -> bool {
        let held = self.sink.lock();
        let Some(sink) = held.as_ref() else { return false };
        match sink.property::<Option<gst::Structure>>("stats") {
            Some(s) => self.heard.live(super::srt_live::answered(&s), std::time::Instant::now()),
            // No statistics on this build: the state is the most this sink
            // can honestly say.
            None => sink.current_state() == gst::State::Playing,
        }
    }

    /// A caller that has heard nothing back for `DOWN_FOR` dials again, for
    /// as long as it takes. A listener is waiting for its callers already.
    fn redial_when_down(&self) -> bool {
        !super::srt_live::is_listener(&self.uri)
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
                let held = self.sink.lock();
                let stats = held.as_ref().and_then(|s| s.property::<Option<gst::Structure>>("stats"));
                Ok(json!({ "stats": stats.map(|s| s.to_string()).unwrap_or_default() }))
            }
            other => Err(unknown_method(&MANIFEST, other, &["stats"])),
        }
    }
}

pub use super::srt_params::validate;
