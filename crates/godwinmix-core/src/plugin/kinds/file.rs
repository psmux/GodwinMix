//! `file/source`: a finite clip. The kind an ad break is built from.
//!
//! It ends with EOS, which is how the break knows to return, and it can be
//! scrubbed, which is why it is the one built in kind declaring `seek`. It gets
//! no `livesync`: a file has neither drift nor gaps, and its timestamps start
//! at zero while the programme's running time is minutes in, so livesync judged
//! every early frame late and an eight second ad lost its first 1.4 seconds.

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
    plugin: "file",
    id: "source",
    kind: ProvideKind::Source,
    api: API_LEVEL,
    description: "A finite clip on disk or over HTTP, scrubbable, ending with EOS",
    uri_schemes: &["file://"],
    // The permissive fallback: anything nothing else claims is treated as a
    // file, which is what `SourceKind::detect` did with its last line.
    rank: 1,
    media: MediaDecl {
        video: StreamMode::Container,
        audio: StreamMode::Container,
        alpha: false,
        thumb: true,
    },
    capabilities: CapabilitySet::new()
        .with(Capability::RestartInPlace)
        .with(Capability::Seek)
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
    if lower.starts_with("file://") {
        // An explicit file URI is claimed properly, not as the fallback.
        return Some(200);
    }
    // Anything else, including a plain file over HTTP, is finite. Claimed at
    // rank 1 so every other kind that recognises the scheme wins first.
    (!uri.trim().is_empty()).then_some(MANIFEST.rank)
}

fn new(req: SourceRequest<'_>) -> Result<Box<dyn Source>> {
    Ok(Box::new(FileSource {
        ctx: req.ctx(),
        pipeline: None,
    }))
}

pub struct FileSource {
    ctx: BuildCtx,
    pipeline: Option<gst::Pipeline>,
}

impl Source for FileSource {
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
        let ends = uridecode(&self.ctx, thumb, false)?;
        self.pipeline = Some(ends.pipeline.clone());
        Ok(ends)
    }

    fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        validate(params)?;
        Ok(Configure::RestartRequired(
            "a clip takes a new path by being reopened".into(),
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
        anyhow::ensure!(v.is_str(), "file/source params.uri must be a string");
    }
    Ok(())
}
