//! The `html/graphic` source itself: its renderer, its picture, its cue.

use super::frames;
use super::layer_parts::{assemble_layer, BuildCtx};
use super::opaque;
use super::params::{self, HtmlParams};
use super::renderer::{self, Renderer};
use super::MANIFEST;
use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::overlay::carrier::Carrier;
use crate::overlay::Layer;
use crate::plugin::source::{unknown_method, Source};
use crate::plugin::{Configure, Health, Hello, Manifest, MediaEnds, PluginState, Ready};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct HtmlSource {
    ctx: BuildCtx,
    params: HtmlParams,
    layer: Arc<Layer>,
    carrier: Option<Arc<Carrier>>,
    /// A design that covers the picture goes to the compositor; see `opaque`.
    opaque: Option<Arc<opaque::Opaque>>,
    renderer: Option<Renderer>,
    /// Whether the programme shows it, as the mixer last said with `cue`.
    on_air: bool,
    /// Set by `stop`: a renderer asked to go did not go by itself.
    stopped: bool,
}

impl HtmlSource {
    pub fn new(ctx: BuildCtx, params: HtmlParams, layer: Arc<Layer>) -> HtmlSource {
        HtmlSource { ctx, params, layer, carrier: None, opaque: None, renderer: None, on_air: false, stopped: false }
    }

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
        let to = frames::Target { layer: self.layer.clone(), carrier, opaque: self.opaque.clone() };
        let page = renderer::Page::template(&page, fps, self.opaque.is_some());
        let r = Renderer::start(&self.ctx.id, &page, &self.ctx.canvas, &self.ctx.browser, to, state)?;
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
        // A design that covers the picture goes to the compositor (`opaque`).
        let ends = if self.params.template.info.opaque {
            let (o, ends) = opaque::Opaque::start(&self.ctx, canvas, thumb)?;
            self.opaque = Some(o);
            ends
        } else {
            let ends = assemble_layer(&self.ctx, thumb, &carrier, &self.layer)?;
            carrier.show(self.layer.picture().as_deref());
            ends
        };
        self.carrier = Some(carrier);
        self.renderer = None;
        self.ensure_renderer()?;
        Ok(ends)
    }

    fn stop(&mut self) -> Result<()> {
        self.stopped = true;
        self.renderer = None;
        if let Some(o) = self.opaque.take() {
            o.stop();
        }
        Ok(())
    }

    /// New words in place, with no reload. A different page is loaded again.
    fn configure(&mut self, params: &Params) -> Result<Configure> {
        let p = params::validate(params)?;
        if p.template.info.opaque != self.params.template.info.opaque {
            return Ok(Configure::RestartRequired("a design that covers the picture is drawn by the compositor, one that does not by the overlay board".into()));
        }
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
        Health::of(if self.renderer.as_ref().is_some_and(|r| !r.ended()) { PluginState::Running } else { PluginState::Starting })
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        match method {
            // Back from NULL: the carrier again, and the renderer if it went.
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
            other => Err(unknown_method(&MANIFEST, other, &["restart", "cue"])),
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
