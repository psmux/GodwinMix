//! `test/source`: a colour bar and a tone, for the conformance harness.
//!
//! It exists so the harness has something to check that needs no network, no
//! file and no browser: `test://smpte` produces video and audio at canvas caps
//! on any machine that has GStreamer at all. Every check the harness makes of a
//! real kind it makes of this one first, so a failure says whether the fault is
//! in the kind or in the harness.

use super::{assemble, BuildCtx, Ingest, KindParts, Wiring};
use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::gstutil::make;
use crate::plugin::source::{unknown_method, Provide, Source, SourceRequest};
use crate::plugin::{
    Capability, CapabilitySet, Configure, Health, Hello, Manifest, MediaDecl, MediaEnds,
    PluginState, ProvideKind, Ready, StreamMode, Tier, API_LEVEL,
};
use anyhow::{Context, Result};
use gstreamer::prelude::*;
use serde_json::Value;

pub const MANIFEST: Manifest = Manifest {
    plugin: "test",
    id: "source",
    kind: ProvideKind::Source,
    api: API_LEVEL,
    description: "A test pattern and a tone, for the conformance harness",
    uri_schemes: &["test://"],
    rank: 256,
    media: MediaDecl {
        video: StreamMode::Raw,
        audio: StreamMode::Raw,
        alpha: false,
        thumb: true,
    },
    capabilities: CapabilitySet::new()
        .with(Capability::RestartInPlace)
        .with(Capability::Health),
    latency_ms: 0,
    tier: Tier::Core,
};

pub const PROVIDE: Provide = Provide { manifest: MANIFEST, claims, make: new };

fn claims(uri: &str) -> Option<u16> {
    uri.trim().to_lowercase().starts_with("test://").then_some(MANIFEST.rank)
}

/// The pattern after `test://`, defaulting to colour bars.
fn pattern(uri: &str) -> String {
    let rest = uri.trim().trim_start_matches("test://").trim_start_matches("TEST://");
    let name = rest.split(['?', '#', '/']).next().unwrap_or("");
    if name.is_empty() { "smpte".to_string() } else { name.to_string() }
}

fn new(req: SourceRequest<'_>) -> Result<Box<dyn Source>> {
    Ok(Box::new(TestSource { pattern: pattern(&req.cfg.uri), ctx: req.ctx(), running: false }))
}

pub struct TestSource {
    ctx: BuildCtx,
    pattern: String,
    running: bool,
}

impl Source for TestSource {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        self.ctx.canvas = hello.canvas;
        Ok(Ready {
            manifest: MANIFEST,
            latency_ms: MANIFEST.latency_ms,
            capabilities: MANIFEST.capabilities,
        })
    }

    fn start(&mut self, canvas: &CanvasCaps, thumb: bool) -> Result<MediaEnds> {
        self.ctx.canvas = canvas.clone();
        let id = &self.ctx.id;
        let vsrc = make("videotestsrc", &format!("{id}-src-test"))?;
        vsrc.set_property("is-live", true);
        vsrc.set_property_from_str("pattern", &self.pattern);
        let asrc = make("audiotestsrc", &format!("{id}-src-tone"))?;
        asrc.set_property("is-live", true);
        // Ten millisecond buffers, which is what the canvas contract asks a
        // plugin for on the audio side.
        crate::probe::set_int(&asrc, "samplesperbuffer", (canvas.sample_rate / 100) as i64);
        let ends = assemble(
            &self.ctx,
            thumb,
            Ingest::default().with([vsrc.clone(), asrc.clone()]).livesync(false),
            |w: &Wiring| {
                vsrc.link(&w.norm.video_entry()).context("linking the test pattern")?;
                asrc.link(&w.norm.audio_entry()).context("linking the test tone")?;
                w.has_video.store(true, std::sync::atomic::Ordering::Relaxed);
                w.has_audio.store(true, std::sync::atomic::Ordering::Relaxed);
                Ok(KindParts::default())
            },
        )?;
        self.running = true;
        Ok(ends)
    }

    fn stop(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }

    fn configure(&mut self, _params: &Params) -> Result<Configure> {
        Ok(Configure::Applied)
    }

    fn health(&self) -> Health {
        Health::of(if self.running { PluginState::Running } else { PluginState::Starting })
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value> {
        match method {
            "restart" => Ok(Value::Null),
            other => Err(unknown_method(&MANIFEST, other, &["restart"])),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::pattern;

    #[test]
    fn the_pattern_comes_off_the_uri_and_defaults_to_bars() {
        assert_eq!(pattern("test://"), "smpte");
        assert_eq!(pattern("test://ball"), "ball");
        assert_eq!(pattern("test://snow?x=1"), "snow");
    }
}
