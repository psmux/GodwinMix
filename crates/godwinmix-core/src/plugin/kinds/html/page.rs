//! `browser/source` with `transparent: true`: any web page over the picture,
//! with its alpha kept.
//!
//! The same renderer an HTML template uses, in graphic mode, but on an
//! address rather than a template: no fields, no cue, the network open. An
//! OGraf graphic's page is drawn this way, and so is a hosted overlay (a
//! chat box, an alert, a scoreboard someone else serves). Only the painted
//! part of the page crosses to the mixer, and only when it changed, so a page
//! that holds still costs next to nothing, where an opaque page source costs a
//! camera's worth whether it moves or not. It has no sound.

use super::renderer::{Page, Renderer};
use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::overlay::carrier::Carrier;
use crate::overlay::Layer;
use crate::plugin::kinds::layer::assemble_layer;
use crate::plugin::kinds::BuildCtx;
use crate::plugin::source::{unknown_method, Source};
use crate::plugin::{Capability, Configure, Health, Hello, Manifest, MediaDecl, MediaEnds, PluginState, Ready, StreamMode};
use anyhow::{Context, Result};
use serde_json::Value;
use std::sync::Arc;

/// `browser/source` as it is when transparent: raw, with alpha, drawn by the
/// board.
pub const MANIFEST: Manifest = Manifest {
    media: MediaDecl { video: StreamMode::Raw, audio: StreamMode::Raw, alpha: true, thumb: true },
    capabilities: crate::plugin::CapabilitySet::new().with(Capability::RestartInPlace).with(Capability::Health).with(Capability::Alpha),
    ..crate::plugin::kinds::browser::MANIFEST
};

/// Whether a browser source's params ask for a transparent page.
pub fn wanted(params: &Params) -> bool {
    params.get("transparent").and_then(|v| v.as_bool()).unwrap_or(false)
}

pub struct TransparentPage {
    ctx: BuildCtx,
    page: Page,
    layer: Arc<Layer>,
    carrier: Option<Arc<Carrier>>,
    renderer: Option<Renderer>,
    stopped: bool,
}

impl TransparentPage {
    pub fn new(ctx: BuildCtx, url: String) -> TransparentPage {
        let page = Page { url, fps: 0, offline: false, designed: false };
        TransparentPage { ctx, page, layer: Layer::new(true), carrier: None, renderer: None, stopped: false }
    }

    fn ensure_renderer(&mut self) -> Result<()> {
        if self.renderer.as_ref().is_some_and(|r| !r.ended()) {
            return Ok(());
        }
        self.renderer = None;
        let carrier = self.carrier.clone().context("the page's pipeline is not built yet")?;
        let r = Renderer::start(&self.ctx.id, &self.page, &self.ctx.canvas, &self.ctx.browser, self.layer.clone(), carrier, "{}".into())?;
        self.renderer = Some(r);
        self.stopped = false;
        Ok(())
    }
}

impl Source for TransparentPage {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        crate::plugin::kinds::browser::validate(&hello.params)?;
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

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        crate::plugin::kinds::browser::validate(params)?;
        Ok(Configure::RestartRequired("a page takes a new address by being loaded again".into()))
    }

    fn health(&self) -> Health {
        let running = self.renderer.as_ref().is_some_and(|r| !r.ended());
        Health::of(if running { PluginState::Running } else { PluginState::Starting })
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value> {
        match method {
            "restart" => {
                if let Some(c) = &self.carrier {
                    c.show(self.layer.picture().as_deref());
                }
                self.ensure_renderer()?;
                Ok(Value::Null)
            }
            "sidecar" => Ok(serde_json::json!({ "sidecar": true, "transparent": true })),
            other => Err(unknown_method(&MANIFEST, other, &["restart", "sidecar"])),
        }
    }

    fn exited(&mut self) -> Option<String> {
        let gone = !self.stopped && self.renderer.as_ref().is_some_and(|r| r.ended());
        gone.then(|| {
            self.stopped = true;
            "the browser renderer drawing the page stopped".to_string()
        })
    }
}
