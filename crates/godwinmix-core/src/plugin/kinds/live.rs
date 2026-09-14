//! `hls/source`: any continuous stream `uridecodebin` opens.
//!
//! HLS, DASH, RTSP, SRT, RTP and plain UDP. The name is the common case; the
//! rank table below is what actually claims a URI. What these have in common
//! and a file does not is that they run indefinitely and drift against our
//! clock, so they go through `livesync`.

use super::{uridecode, BuildCtx};
use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::plugin::source::{unknown_method, Provide, Source, SourceRequest};
use crate::plugin::{
    Capability, CapabilitySet, Configure, Health, Hello, Manifest, MediaDecl, MediaEnds,
    PluginState, ProvideKind, Ready, StreamMode, Tier, API_LEVEL,
};
use anyhow::Result;
use gstreamer as gst;
use serde_json::Value;

pub const MANIFEST: Manifest = Manifest {
    plugin: "hls",
    id: "source",
    kind: ProvideKind::Source,
    api: API_LEVEL,
    description: "A continuous stream: HLS, DASH, RTSP, SRT, RTP or UDP",
    uri_schemes: &["rtsp://", "rtsps://", "srt://", "udp://", "rtp://"],
    rank: 200,
    media: MediaDecl {
        video: StreamMode::Container,
        audio: StreamMode::Container,
        alpha: false,
        thumb: true,
    },
    capabilities: CapabilitySet::new()
        .with(Capability::RestartInPlace)
        .with(Capability::Health),
    latency_ms: 0,
    tier: Tier::Core,
};

pub const PROVIDE: Provide = Provide {
    manifest: MANIFEST,
    claims,
    make: new,
};

fn claims(uri: &str) -> Option<u16> {
    let lower = uri.trim().to_lowercase();
    if ["rtsp://", "rtsps://", "srt://", "udp://", "rtp://"]
        .iter()
        .any(|p| lower.starts_with(p))
    {
        return Some(MANIFEST.rank);
    }
    // Playlist manifests are live regardless of being fetched over HTTP. The
    // query string is cut first: a signed CDN URL ends in a token, not in the
    // extension that says what it is.
    let path = lower.split(['?', '#']).next().unwrap_or(&lower);
    (path.ends_with(".m3u8") || path.ends_with(".mpd")).then_some(MANIFEST.rank)
}

fn new(req: SourceRequest<'_>) -> Result<Box<dyn Source>> {
    Ok(Box::new(LiveSource {
        ctx: req.ctx(),
        pipeline: None,
    }))
}

pub struct LiveSource {
    ctx: BuildCtx,
    pipeline: Option<gst::Pipeline>,
}

impl Source for LiveSource {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        validate(&hello.params)?;
        self.ctx.canvas = hello.canvas;
        Ok(Ready {
            manifest: MANIFEST,
            latency_ms: MANIFEST.latency_ms,
            capabilities: MANIFEST.capabilities,
        })
    }

    fn start(&mut self, canvas: &CanvasCaps, thumb: bool) -> Result<MediaEnds> {
        self.ctx.canvas = canvas.clone();
        let ends = uridecode(&self.ctx, thumb, true)?;
        self.pipeline = Some(ends.pipeline.clone());
        Ok(ends)
    }

    fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        validate(params)?;
        Ok(Configure::RestartRequired(
            "a stream takes a new address by being reopened".into(),
        ))
    }

    fn health(&self) -> Health {
        Health::of(if self.pipeline.is_some() {
            PluginState::Running
        } else {
            PluginState::Starting
        })
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value> {
        match method {
            "restart" => Ok(Value::Null),
            other => Err(unknown_method(&MANIFEST, other, &["restart"])),
        }
    }
}

pub fn validate(params: &Params) -> Result<()> {
    if let Some(v) = params.get("uri") {
        anyhow::ensure!(v.is_str(), "hls/source params.uri must be a string");
    }
    Ok(())
}
