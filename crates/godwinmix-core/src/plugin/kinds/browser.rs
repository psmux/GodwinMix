//! `browser/source`: a rendered web page.
//!
//! Two paths, one kind. With a sidecar configured the page is drawn by a
//! separate process writing Matroska to stdout, which makes it an exec source
//! in every respect and is the out of process plugin we already ship. Without
//! one it falls back to `wpesrc`, which renders in our own process and draws
//! into GL memory, so the frames come back to system memory before the
//! normaliser can touch them.

use super::exec::ExecProcess;
use super::page_control::PageControl;
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

pub const PROVIDE: Provide = Provide { manifest: MANIFEST, claims, make: new };

fn claims(uri: &str) -> Option<u16> {
    crate::input::web_url(uri).map(|_| MANIFEST.rank)
}

fn new(req: SourceRequest<'_>) -> Result<Box<dyn Source>> {
    // A transparent page is drawn in graphic mode and over the picture by the
    // board; see `html::page`.
    if super::html::page::wanted(&req.cfg.effective_params()) {
        let url = crate::input::web_url(req.cfg.uri.as_str()).context("not a web source url")?;
        return Ok(Box::new(super::html::page::TransparentPage::new(req.ctx(), url)));
    }
    let sidecar = ExecSpec::browser(req.cfg.uri.as_str(), req.canvas, req.browser)?;
    let gpu = wants_gpu(&req.cfg.effective_params());
    let page = PageControl::default();
    let process = sidecar.map(|mut spec| {
        // A control line, so the operator can use the page from the mixer's
        // interface. Nothing is written on it until somebody does.
        spec.pipe_stdin = true;
        spec.argv.insert(1, "--control".into());
        if gpu {
            spec.argv.insert(1, "--gpu".into());
        }
        let mut process = ExecProcess::new(spec);
        process.on_line = Some(page.watch());
        process
    });
    Ok(Box::new(BrowserSource { ctx: req.ctx(), process, page, running: false }))
}

pub struct BrowserSource {
    ctx: BuildCtx,
    /// Some when a sidecar renders the page. None falls back to `wpesrc`.
    process: Option<ExecProcess>,
    /// The operator's clicks and keys on the way down, and where the page is
    /// on the way up. See `page_control`.
    page: PageControl,
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
        self.page.attach(process.child.as_mut().and_then(|c| c.take_stdin()));
        let decode = ExecProcess::decoder(&id)?;
        assemble(
            &self.ctx,
            thumb,
            Ingest::default().with([src.clone(), decode.clone()]).livesync(false),
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
        // wpesrc draws into GL memory, so there is no software only path.
        let missing = crate::setup::system::absent(&["glcolorconvert", "gldownload"]);
        if !missing.is_empty() {
            return Err(crate::setup::system::missing_error("Showing web pages", crate::setup::system::GL, &missing));
        }
        let src = make_web_source(id, &ctx.cfg.uri)?;
        let gl_convert = make("glcolorconvert", &format!("{id}-gl-conv"))?;
        let gl_download = make("gldownload", &format!("{id}-gl-dl"))?;
        let web_caps =
            gstutil::capsfilter(&format!("{id}-web-caps"), &web_render_caps(&ctx.canvas))?;
        assemble(
            ctx,
            thumb,
            Ingest::default()
                .with([src.clone(), gl_convert.clone(), gl_download.clone(), web_caps.clone()])
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
                let entry = gl_convert.static_pad("sink").context("gl chain has no sink pad")?;
                out.link(&entry).context("linking web source video")?;
                w.has_video.store(true, std::sync::atomic::Ordering::Relaxed);
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
        Ok(Ready { manifest: MANIFEST, latency_ms: MANIFEST.latency_ms, capabilities })
    }

    fn start(&mut self, canvas: &CanvasCaps, thumb: bool) -> Result<MediaEnds> {
        self.ctx.canvas = canvas.clone();
        let ends =
            if self.process.is_some() { self.start_sidecar(thumb)? } else { self.start_wpe(thumb)? };
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
        Ok(Configure::RestartRequired("a page takes a new address by being loaded again".into()))
    }

    fn health(&self) -> Health {
        Health::of(if self.running { PluginState::Running } else { PluginState::Starting })
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        match method {
            "restart" => match &mut self.process {
                Some(p) => {
                    p.respawn(&self.ctx.id)?;
                    self.page.attach(p.child.as_mut().and_then(|c| c.take_stdin()));
                    Ok(json!({ "respawned": true }))
                }
                None => Ok(Value::Null),
            },
            "sidecar" => Ok(json!({ "sidecar": self.sidecar() })),
            "page.state" => Ok(serde_json::to_value(self.page.state())?),
            "page" => self.page_lines(params),
            "page.go" => self.page_go(params),
            other => Err(unknown_method(&MANIFEST, other, &["restart", "sidecar", "page", "page.go", "page.state"])),
        }
    }
}

impl BrowserSource {
    fn page_process(&self) -> Result<()> {
        anyhow::ensure!(
            self.process.is_some(),
            "this page is drawn by GStreamer's own web renderer, which takes no clicks or keys.              Install the browser renderer (Settings, Web pages) and add the page again to use it"
        );
        Ok(())
    }

    /// `{"lines": [...]}`: control lines for the renderer, already checked.
    fn page_lines(&mut self, params: Value) -> Result<Value> {
        self.page_process()?;
        let lines: Vec<String> = serde_json::from_value(params.get("lines").cloned().unwrap_or_default())?;
        let (sent, dropped) = self.page.send(&self.ctx.id, lines)?;
        Ok(json!({ "sent": sent, "dropped": dropped }))
    }

    /// `{"url": ...}`: go there now, and come back there after a restart.
    fn page_go(&mut self, params: Value) -> Result<Value> {
        self.page_process()?;
        let url = params.get("url").and_then(Value::as_str).context("page.go needs a url")?.to_string();
        let line = godwinmix_protocol::page::navigate_line(godwinmix_protocol::page::PageAction::Go, Some(&url));
        self.page.send(&self.ctx.id, vec![line])?;
        if let Some(p) = &mut self.process {
            if let Some(i) = p.spec.argv.iter().position(|a| a == "--url") {
                if let Some(v) = p.spec.argv.get_mut(i + 1) {
                    *v = url.clone();
                }
            }
        }
        self.page.going_to(&url);
        Ok(json!({ "url": url }))
    }
}

/// `params.gpu`: the renderer may use the GPU, which is the only way it
/// plays HEVC. Off unless asked, like everything else the mixer could run.
pub fn wants_gpu(params: &Params) -> bool {
    params.get("gpu").and_then(|v| v.as_bool()).unwrap_or(false)
}

pub fn validate(params: &Params) -> Result<()> {
    for (key, value) in params {
        match key.as_str() {
            "uri" | "url" => {
                anyhow::ensure!(value.is_str(), "browser/source params.{key} must be a string");
            }
            "transparent" | "gpu" => {
                anyhow::ensure!(value.is_bool(), "browser/source params.{key} is true or false");
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

#[cfg(test)]
#[path = "browser_tests.rs"]
mod tests;
