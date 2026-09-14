//! `exec/source`: a command line whose process writes media to stdout.
//!
//! The universal escape hatch, and the seed of the out of process plugin
//! protocol: media on stdout in any container `decodebin` opens, events on
//! stderr, a process group for the lifecycle. A sidecar source will be this
//! kind with a manifest handshake and a command channel on stdin.
//!
//! It gets no `livesync`. The process already paces its output, and its
//! timestamps count from its own start: livesync judged every one of those
//! frames late against our clock, dropped the lot, and repeated the first black
//! frame forever while the audio played on.

use super::{assemble, BuildCtx, Ingest, KindParts, Wiring};
use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::gstutil::make;
use crate::input::{attach_exec_stdout, make_exec_source, spawn_exec, ExecChild, ExecSpec};
use crate::plugin::source::{unknown_method, Provide, Source, SourceRequest};
use crate::plugin::{
    Capability, CapabilitySet, Configure, Health, Hello, Manifest, MediaDecl, MediaEnds,
    PluginState, ProvideKind, Ready, StreamMode, Tier, API_LEVEL,
};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::{json, Value};
use tracing::{debug, warn};

pub const MANIFEST: Manifest = Manifest {
    plugin: "exec",
    id: "source",
    kind: ProvideKind::Source,
    api: API_LEVEL,
    description: "A command whose process writes a container to stdout",
    uri_schemes: &["exec:", "exec://"],
    rank: 256,
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
    crate::input::exec_command(uri).map(|_| MANIFEST.rank)
}

fn new(req: SourceRequest<'_>) -> Result<Box<dyn Source>> {
    let spec = ExecSpec::from_uri(&req.cfg.uri, req.allow_exec)?;
    Ok(Box::new(ExecSource::new(req.ctx(), spec)))
}

/// The process half of an exec or browser source, shared by both kinds.
///
/// Held separately from the kinds so `browser/source` running a sidecar is
/// literally this code rather than a copy of it.
pub struct ExecProcess {
    pub spec: ExecSpec,
    pub child: Option<ExecChild>,
    pub src: Option<gst::Element>,
}

impl ExecProcess {
    pub fn new(spec: ExecSpec) -> Self {
        Self {
            spec,
            child: None,
            src: None,
        }
    }

    /// Build the `fdsrc` (or, on Windows, the reader thread's `appsrc`) and
    /// spawn the process behind it.
    pub fn build_src(&mut self, id: &str) -> Result<gst::Element> {
        let (el, child) = make_exec_source(id, &self.spec)?;
        self.child = Some(child);
        self.src = Some(el.clone());
        Ok(el)
    }

    /// A decoder for whatever container the process writes. `decodebin` does
    /// the demuxing and the decoding, and respects the ranks the hardware
    /// probe set.
    pub fn decoder(id: &str) -> Result<gst::Element> {
        make("decodebin", &format!("{id}-decode"))
    }

    /// Kill the process. Letting go of the child is what kills it, and the
    /// drop runs the process group teardown.
    pub fn kill(&mut self, id: &str) {
        if self.child.take().is_some() {
            debug!(source = %id, "stopping exec child process");
        }
    }

    /// Start the process again and hand the new fd to the same source element,
    /// which is what `restart-in-place` means for this kind.
    pub fn respawn(&mut self, id: &str) -> Result<()> {
        let Some(src) = self.src.clone() else {
            anyhow::bail!("{id} has no source element to hand a new pipe to; build it first");
        };
        self.kill(id);
        match spawn_exec(id, &self.spec) {
            Ok((out, child, stderr)) => {
                let stdout = attach_exec_stdout(id, &src, out);
                self.child = Some(ExecChild::new(child, self.spec.env.clone(), stdout, stderr));
                Ok(())
            }
            Err(e) => {
                warn!(source = %id, ?e, "could not restart exec source");
                Err(e)
            }
        }
    }
}

pub struct ExecSource {
    ctx: BuildCtx,
    process: ExecProcess,
    running: bool,
}

impl ExecSource {
    pub fn new(ctx: BuildCtx, spec: ExecSpec) -> Self {
        Self {
            ctx,
            process: ExecProcess::new(spec),
            running: false,
        }
    }
}

impl Source for ExecSource {
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
        let id = self.ctx.id.clone();
        let src = self.process.build_src(&id)?;
        let decode = ExecProcess::decoder(&id)?;
        let ends = assemble(
            &self.ctx,
            thumb,
            Ingest::default()
                .with([src.clone(), decode.clone()])
                .livesync(false),
            |w: &Wiring| {
                gst::Element::link(&src, &decode).context("linking exec source to decoder")?;
                w.route(&decode, w.norm.video_entry(), w.norm.audio_entry());
                Ok(KindParts::default())
            },
        )?;
        self.running = true;
        Ok(ends)
    }

    fn stop(&mut self) -> Result<()> {
        self.process.kill(&self.ctx.id);
        self.running = false;
        Ok(())
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        validate(params)?;
        Ok(Configure::RestartRequired(
            "an exec source takes a new command line by respawning".into(),
        ))
    }

    fn health(&self) -> Health {
        Health::of(if self.running {
            PluginState::Running
        } else {
            PluginState::Starting
        })
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value> {
        match method {
            "restart" => {
                self.process.respawn(&self.ctx.id)?;
                Ok(json!({ "respawned": true }))
            }
            other => Err(unknown_method(&MANIFEST, other, &["restart"])),
        }
    }
}

pub fn validate(params: &Params) -> Result<()> {
    for (key, value) in params {
        match key.as_str() {
            "uri" | "command" => {
                anyhow::ensure!(value.is_str(), "exec/source params.{key} must be a string");
            }
            "env" => {
                anyhow::ensure!(
                    value.is_table(),
                    "exec/source params.env must be a table of name to value"
                );
            }
            _ => {}
        }
    }
    Ok(())
}
