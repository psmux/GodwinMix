//! `browser/source`: a rendered web page.
//!
//! Two paths, one kind. With a sidecar configured the page is drawn by a
//! separate process writing Matroska to stdout, which makes it an exec source
//! in every respect and is the out of process plugin we already ship. Without
//! one it falls back to `wpesrc`, which renders in our own process and draws
//! into GL memory, so the frames come back to system memory before the
//! normaliser can touch them.

use super::exec::ExecProcess;
use super::{assemble, BuildCtx, Ingest, KindParts, Wiring};
use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::gstutil::{self, make};
use crate::input::{make_web_source, web_render_caps, ExecSpec};
use crate::plugin::source::{unknown_method, Provide, Source, SourceRequest};
use crate::plugin::{
    Capability, CapabilitySet, Configure, Health, Hello, Manifest, MediaDecl, MediaEnds,
    PluginState, ProvideKind, Ready, StreamMode, Tier, API_LEVEL,
};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::{json, Value};

pub const MANIFEST: Manifest = Manifest {
    plugin: "browser",
    id: "source",
    kind: ProvideKind::Source,
    api: API_LEVEL,
    description: "A web page rendered as a live source, by the sidecar or by wpesrc",
    uri_schemes: &["web://", "web+http://", "web+https://"],
    rank: 250,
    media: MediaDecl {
        video: StreamMode::Container,
        audio: StreamMode::Container,
        alpha: false,
        thumb: true,
    },
    // No `restart-in-place` when the page is drawn by a sidecar: see the
    // comment on `Mixer::rebuild_source`. The capability is added back at
    // `initialize` for the wpesrc path, which restarts cleanly.
    capabilities: CapabilitySet::new().with(Capability::Health),
    latency_ms: 0,
    tier: Tier::Core,
};

pub const PROVIDE: Provide = Provide {
    manifest: MANIFEST,
    claims,
    make: new,
};

fn claims(uri: &str) -> Option<u16> {
    crate::input::web_url(uri).map(|_| MANIFEST.rank)
}

fn new(req: SourceRequest<'_>) -> Result<Box<dyn Source>> {
    let sidecar = ExecSpec::browser(req.cfg.uri.as_str(), req.canvas, req.browser)?;
    Ok(Box::new(BrowserSource {
        ctx: req.ctx(),
        process: sidecar.map(ExecProcess::new),
        running: false,
    }))
}

pub struct BrowserSource {
    ctx: BuildCtx,
    /// Some when a sidecar renders the page. None falls back to `wpesrc`.
    process: Option<ExecProcess>,
    running: bool,
}

impl BrowserSource {
    /// Whether this instance is drawn out of process.
    pub fn sidecar(&self) -> bool {
        self.process.is_some()
    }

    fn start_sidecar(&mut self, thumb: bool) -> Result<MediaEnds> {
        let id = self.ctx.id.clone();
        let process = self.process.as_mut().expect("checked by the caller");
        let src = process.build_src(&id)?;
        let decode = ExecProcess::decoder(&id)?;
        assemble(
            &self.ctx,
            thumb,
            Ingest::default()
                .with([src.clone(), decode.clone()])
                .livesync(false),
            |w: &Wiring| {
                gst::Element::link(&src, &decode).context("linking the sidecar to the decoder")?;
                w.route(&decode, w.norm.video_entry(), w.norm.audio_entry());
                Ok(KindParts::default())
            },
        )
    }

    /// The in process path. `wpesrc` hands out RGBA in GL memory, so it has to
    /// come back to system memory before the normaliser can touch it, and the
    /// render capsfilter sits after the download so the size it asks for
    /// negotiates back upstream to the renderer.
    fn start_wpe(&mut self, thumb: bool) -> Result<MediaEnds> {
        let ctx = &self.ctx;
        let id = &ctx.id;
        anyhow::ensure!(
            crate::probe::exists("glcolorconvert") && crate::probe::exists("gldownload"),
            "rendering web pages needs the GStreamer OpenGL elements \
             (gstreamer1.0-gl on Debian and Ubuntu). wpesrc draws into GL memory, \
             so there is no software-only path."
        );
        let src = make_web_source(id, &ctx.cfg.uri)?;
        let gl_convert = make("glcolorconvert", &format!("{id}-gl-conv"))?;
        let gl_download = make("gldownload", &format!("{id}-gl-dl"))?;
        let web_caps =
            gstutil::capsfilter(&format!("{id}-web-caps"), &web_render_caps(&ctx.canvas))?;
        assemble(
            ctx,
            thumb,
            Ingest::default()
                .with([
                    src.clone(),
                    gl_convert.clone(),
                    gl_download.clone(),
                    web_caps.clone(),
                ])
                .livesync(true),
            |w: &Wiring| {
                gst::Element::link_many([
                    &gl_convert,
                    &gl_download,
                    &web_caps,
                    &w.norm.video_entry(),
                ])
                .context("linking the web render chain")?;
                // `wpesrc`'s video pad is a static pad named `video`, not
                // `src`, and it is always present. Its audio arrives later on
                // `audio_%u`, which the routing below picks up by caps.
                let out = src
                    .static_pad("video")
                    .context("wpesrc has no `video` pad; the plugin version may differ")?;
                let entry = gl_convert
                    .static_pad("sink")
                    .context("gl chain has no sink pad")?;
                out.link(&entry).context("linking web source video")?;
                w.has_video
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                w.route(&src, gl_convert.clone(), w.norm.audio_entry());
                Ok(KindParts::default())
            },
        )
    }
}

impl Source for BrowserSource {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        validate(&hello.params)?;
        self.ctx.canvas = hello.canvas;
        let mut capabilities = MANIFEST.capabilities;
        // A page in our own process comes back in place. One in a sidecar does
        // not: brought back in place its audio mixer spun on a failing latency
        // query, so it is rebuilt from nothing instead.
        capabilities.set(Capability::RestartInPlace, self.process.is_none());
        Ok(Ready {
            manifest: MANIFEST,
            latency_ms: MANIFEST.latency_ms,
            capabilities,
        })
    }

    fn start(&mut self, canvas: &CanvasCaps, thumb: bool) -> Result<MediaEnds> {
        self.ctx.canvas = canvas.clone();
        let ends = if self.process.is_some() {
            self.start_sidecar(thumb)?
        } else {
            self.start_wpe(thumb)?
        };
        self.running = true;
        Ok(ends)
    }

    fn stop(&mut self) -> Result<()> {
        if let Some(p) = &mut self.process {
            p.kill(&self.ctx.id);
        }
        self.running = false;
        Ok(())
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        validate(params)?;
        Ok(Configure::RestartRequired(
            "a page takes a new address by being loaded again".into(),
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
            "restart" => match &mut self.process {
                Some(p) => {
                    p.respawn(&self.ctx.id)?;
                    Ok(json!({ "respawned": true }))
                }
                None => Ok(Value::Null),
            },
            "sidecar" => Ok(json!({ "sidecar": self.sidecar() })),
            other => Err(unknown_method(&MANIFEST, other, &["restart", "sidecar"])),
        }
    }
}

pub fn validate(params: &Params) -> Result<()> {
    for (key, value) in params {
        match key.as_str() {
            "uri" | "url" => {
                anyhow::ensure!(
                    value.is_str(),
                    "browser/source params.{key} must be a string"
                );
            }
            "superimpose" => {
                let s = value.as_str().unwrap_or_default();
                anyhow::ensure!(
                    matches!(s, "off" | "auto"),
                    "browser/source params.superimpose must be off or auto, not `{s}`"
                );
            }
            _ => {}
        }
    }
    Ok(())
}
