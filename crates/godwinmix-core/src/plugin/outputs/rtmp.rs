//! `rtmp/output`: FLV over RTMP, which is what every CDN ingest takes.
//!
//! What used to be hardcoded in `OutputSlot::spin_up`, moved behind the trait
//! with nothing lost. The FLV specifics that were learned the hard way are all
//! still here, and they are all here rather than spread through the core.

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
    plugin: "rtmp",
    id: "output",
    kind: ProvideKind::Output,
    api: API_LEVEL,
    description: "FLV over RTMP or RTMPS, the ingest every CDN takes",
    uri_schemes: &["rtmp://", "rtmps://"],
    rank: 240,
    media: MediaDecl {
        video: StreamMode::Container,
        audio: StreamMode::Container,
        alpha: false,
        thumb: false,
    },
    capabilities: CapabilitySet::new().with(Capability::KeyframeRequest),
    latency_ms: 0,
    tier: Tier::Core,
};

pub const PROVIDE: OutputProvide = OutputProvide { manifest: MANIFEST, claims, make: new };

fn claims(uri: &str) -> Option<u16> {
    let lower = uri.trim().to_lowercase();
    (lower.starts_with("rtmp://") || lower.starts_with("rtmps://")).then_some(MANIFEST.rank)
}

fn new(cfg: &OutputConfig) -> Result<Box<dyn Output>> {
    Ok(Box::new(RtmpOutput { uri: cfg.uri.clone(), sink: Mutex::new(None) }))
}

pub struct RtmpOutput {
    uri: String,
    sink: Mutex<Option<gst::Element>>,
}

impl Output for RtmpOutput {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        validate(&hello.params)?;
        if let Some(u) = hello.params.get("uri").and_then(|v| v.as_str()) {
            self.uri = u.to_string();
        }
        anyhow::ensure!(!self.uri.trim().is_empty(), "rtmp/output needs an address in params.uri");
        Ok(Ready {
            manifest: MANIFEST,
            latency_ms: MANIFEST.latency_ms,
            capabilities: MANIFEST.capabilities,
        })
    }

    fn build(
        &mut self,
        ctx: &OutputCtx<'_>,
        video: &gst::Element,
        audio: &gst::Element,
    ) -> Result<()> {
        let (id, gen) = (ctx.id, ctx.generation);
        let mux = make("flvmux", &format!("out-{id}-mux-{gen}"))?;
        mux.set_property("streamable", true);
        // Each connection is its own FLV stream and starts at zero. The
        // continuity a viewer cares about is in the encoded bitstream, which is
        // never interrupted, not in the container timestamps.
        crate::probe::set_enum(&mux, "start-time-selection", "first");
        crate::probe::set_bool(&mux, "enforce-increasing-timestamps", true);
        crate::probe::set_bool(&mux, "skip-backwards-streams", true);

        let sink = make("rtmp2sink", &format!("out-{id}-rtmp-{gen}"))?;
        sink.set_property("location", &self.uri);
        crate::probe::set_bool(&sink, "async-connect", true);
        // The encoder already produced this in real time. Making the sink wait
        // on the clock a second time only adds latency.
        crate::probe::set_bool(&sink, "sync", false);
        crate::probe::set_bool(&sink, "async", false);

        ctx.pipeline.add_many([&mux, &sink]).context("adding the rtmp muxer and sink")?;
        link_to_mux(video, &mux, &["video"])?;
        link_to_mux(audio, &mux, &["audio"])?;
        mux.link(&sink).context("linking muxer to rtmp sink")?;
        *self.sink.lock() = Some(sink);
        Ok(())
    }

    /// True once the RTMP handshake has completed on the current pipeline.
    ///
    /// The obvious signal, a buffer reaching the sink, is wrong: `rtmp2sink`
    /// runs with `async-connect`, so it accepts buffers immediately and
    /// performs the handshake in the background. Data flowing therefore says
    /// nothing about whether the far end ever answered, and an operator would
    /// see "live" against a destination that was refusing us.
    ///
    /// `out-chunk-size` is zero until the RTMP handshake negotiates it, which
    /// makes it a real connection signal.
    fn connected(&self) -> bool {
        self.sink
            .lock()
            .as_ref()
            .and_then(|s| s.property::<Option<gst::Structure>>("stats"))
            .and_then(|s| s.get::<u32>("out-chunk-size").ok())
            .unwrap_or(0)
            > 0
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        validate(params)?;
        Ok(Configure::RestartRequired("an rtmp output takes a new address by reconnecting".into()))
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

pub fn validate(params: &Params) -> Result<()> {
    if let Some(v) = params.get("uri") {
        anyhow::ensure!(v.is_str(), "rtmp/output params.uri must be a string");
    }
    Ok(())
}
