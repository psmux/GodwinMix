//! `html/graphic`: a designed graphic that moves, an HTML template drawn
//! by the browser renderer over the picture with its alpha kept.
//!
//! The overlay board draws it like a text or an SVG template: a layer whose
//! picture is the part of the page with something in it, a carrier through
//! the ordinary source path, silence for its sound. What is different is
//! where the picture comes from: a renderer process in graphic mode, which
//! sends a picture only when the page painted one. The page is told its
//! fields and whether it is in or out on the renderer's stdin, so a field
//! changed on air is shown with no reload, and the programme taking an item
//! that shows it plays the page's own way in (`Capability::Cue`).

pub mod frames;
pub mod page;
pub mod params;
pub mod renderer;

use super::layer::assemble_layer;
use super::BuildCtx;
use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::overlay::carrier::Carrier;
use crate::overlay::Layer;
use crate::plugin::source::{unknown_method, Provide, Source, SourceRequest};
use crate::plugin::{Capability, CapabilitySet, Configure, Health, Hello, Manifest, MediaDecl, MediaEnds, PluginState, ProvideKind, Ready, StreamMode, Tier, API_LEVEL};
use anyhow::{Context, Result};
use params::HtmlParams;
use renderer::Renderer;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub use params::schema;

pub const MANIFEST: Manifest = Manifest {
    plugin: "html",
    id: "graphic",
    kind: ProvideKind::Source,
    api: API_LEVEL,
    description: "A designed graphic that moves: an HTML template with named fields, drawn by the browser renderer over the picture with its transparency",
    uri_schemes: &["html:"],
    rank: 230,
    media: MediaDecl { video: StreamMode::Raw, audio: StreamMode::Raw, alpha: true, thumb: true },
    capabilities: CapabilitySet::new().with(Capability::RestartInPlace).with(Capability::Health).with(Capability::Alpha).with(Capability::Cue),
    latency_ms: 0,
    tier: Tier::Core,
};

pub const PROVIDE: Provide = Provide { manifest: MANIFEST, claims, make: new };

fn claims(uri: &str) -> Option<u16> {
    crate::graphics::html::name_in(uri).map(|_| MANIFEST.rank)
}

fn new(req: SourceRequest<'_>) -> Result<Box<dyn Source>> {
    let params = params::validate(&req.cfg.effective_params())?;
    Ok(Box::new(HtmlSource { ctx: req.ctx(), params, layer: Layer::new(true), carrier: None, renderer: None, on_air: false, stopped: false }))
}

pub struct HtmlSource {
    ctx: BuildCtx,
    params: HtmlParams,
    layer: Arc<Layer>,
    carrier: Option<Arc<Carrier>>,
    renderer: Option<Renderer>,
    /// Whether an item showing this is on the programme, as the mixer last
    /// said with `cue`.
    on_air: bool,
    /// Set by `stop`, so a renderer that was asked to go is not reported as
    /// one that went by itself.
    stopped: bool,
}

impl HtmlSource {
    /// Start the renderer if it is not running.
    fn ensure_renderer(&mut self) -> Result<()> {
        if self.renderer.as_ref().is_some_and(|r| !r.ended()) {
            return Ok(());
        }
        self.renderer = None;
        let carrier = self.carrier.clone().context("the graphic's pipeline is not built yet")?;
        let page = self.params.template.file.clone().context("the template has no file to load")?;
        let state = self.params.state(self.on_air);
        let fps = if self.params.fps > 0 { self.params.fps } else { self.params.template.fps.unwrap_or(0) };
        let r = Renderer::start(&self.ctx.id, &renderer::Page::template(&page, fps), &self.ctx.canvas, &self.ctx.browser, self.layer.clone(), carrier, state)?;
        self.renderer = Some(r);
        self.stopped = false;
        Ok(())
    }

    fn tell(&self) {
        if let Some(r) = &self.renderer {
            r.send(self.params.state(self.on_air));
        }
    }
}

impl Source for HtmlSource {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        self.params = params::validate(&hello.params)?;
        self.ctx.canvas = hello.canvas;
        Ok(Ready { manifest: MANIFEST, latency_ms: 0, capabilities: MANIFEST.capabilities })
    }

    fn start(&mut self, canvas: &CanvasCaps, thumb: bool) -> Result<MediaEnds> {
        self.ctx.canvas = canvas.clone();
        let carrier = Arc::new(Carrier::build(&self.ctx.id, canvas)?);
        let ends = assemble_layer(&self.ctx, thumb, &carrier, &self.layer)?;
        carrier.show(self.layer.picture().as_deref());
        self.carrier = Some(carrier);
        self.renderer = None;
        self.ensure_renderer()?;
        Ok(ends)
    }

    fn stop(&mut self) -> Result<()> {
        self.stopped = true;
        self.renderer = None;
        Ok(())
    }

    /// New words in place, with no reload. A different page is loaded again.
    fn configure(&mut self, params: &Params) -> Result<Configure> {
        let p = params::validate(params)?;
        let other_page = p.template != self.params.template || p.fps != self.params.fps;
        self.params = p;
        if other_page && self.carrier.is_some() {
            self.renderer = None;
            self.ensure_renderer()?;
        } else {
            self.tell();
        }
        Ok(Configure::Applied)
    }

    fn health(&self) -> Health {
        let running = self.renderer.as_ref().is_some_and(|r| !r.ended());
        Health::of(if running { PluginState::Running } else { PluginState::Starting })
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        match method {
            // The pipeline went to NULL and back: show the carrier again, and
            // start the renderer if it had gone.
            "restart" => {
                if let Some(c) = &self.carrier {
                    c.show(self.layer.picture().as_deref());
                }
                self.ensure_renderer()?;
                Ok(Value::Null)
            }
            // The mixer: an item showing this went on or came off the air.
            "cue" => {
                self.on_air = params.get("on_air").and_then(Value::as_bool).unwrap_or(false);
                self.tell();
                Ok(json!({ "in": self.params.is_in(self.on_air) }))
            }
            "state" => {
                let frames = self.renderer.as_ref().map(|r| r.feed.frames.load(Ordering::Relaxed)).unwrap_or(0);
                let state: Value = serde_json::from_str(&self.params.state(self.on_air)).unwrap_or(Value::Null);
                Ok(json!({ "state": state, "frames": frames, "on_air": self.on_air }))
            }
            other => Err(unknown_method(&MANIFEST, other, &["restart", "cue", "state"])),
        }
    }

    fn exited(&mut self) -> Option<String> {
        let gone = !self.stopped && self.renderer.as_ref().is_some_and(|r| r.ended());
        gone.then(|| {
            self.stopped = true;
            "the browser renderer drawing the graphic stopped".to_string()
        })
    }
}

#[cfg(test)]
mod tests;
