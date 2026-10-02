//! One `Source` for every kind whose picture is rendered in this process
//! rather than decoded: text, ticker, SVG. Each says what its params are and
//! how to render them; this holds the layer, the carrier and the worker, and
//! answers the core.

use super::{layer::assemble_layer, BuildCtx};
use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::overlay::carrier::Carrier;
use crate::overlay::worker::{self, Msg, Rendered};
use crate::overlay::Layer;
use crate::plugin::source::{unknown_method, Source, SourceRequest};
use crate::plugin::{Configure, Health, Hello, Manifest, MediaEnds, PluginState, Ready};
use anyhow::Result;
use serde_json::Value;
use std::sync::mpsc::Sender;
use std::sync::Arc;

/// What a rendered kind provides.
pub trait Rendering: Clone + Send + Sized + 'static {
    fn manifest() -> &'static Manifest;
    /// Read and check params, naming the field that is wrong.
    fn validate(params: &Params) -> Result<Self>;
    /// Render at `drawn`, the size the board draws the item at, or at the
    /// kind's own size before it has been drawn.
    fn render(&self, drawn: Option<(u32, u32)>) -> Result<Rendered>;
    /// Whether changing from `old` to this starts a crawl again from its edge.
    fn restarts(&self, _old: &Self) -> bool {
        false
    }
}

/// The factory a `Provide` points at.
pub fn make<P: Rendering>(req: SourceRequest<'_>) -> Result<Box<dyn Source>> {
    let params = P::validate(&req.cfg.effective_params())?;
    Ok(Box::new(RenderedSource { ctx: req.ctx(), params, layer: Layer::new(true), carrier: None, worker: None }))
}

pub struct RenderedSource<P: Rendering> {
    ctx: BuildCtx,
    params: P,
    layer: Arc<Layer>,
    carrier: Option<Arc<Carrier>>,
    worker: Option<Sender<Msg<P>>>,
}

/// A source dropped without being stopped still lets its render thread go.
/// The layer keeps a sender for the board's sizes, so the channel alone would
/// never close.
impl<P: Rendering> Drop for RenderedSource<P> {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

impl<P: Rendering> Source for RenderedSource<P> {
    fn manifest(&self) -> &Manifest {
        P::manifest()
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        self.params = P::validate(&hello.params)?;
        self.ctx.canvas = hello.canvas;
        let m = P::manifest();
        Ok(Ready { manifest: *m, latency_ms: 0, capabilities: m.capabilities })
    }

    fn start(&mut self, canvas: &CanvasCaps, thumb: bool) -> Result<MediaEnds> {
        self.ctx.canvas = canvas.clone();
        let carrier = Arc::new(Carrier::build(&self.ctx.id, canvas)?);
        let ends = assemble_layer(&self.ctx, thumb, &carrier, &self.layer)?;
        carrier.show(self.layer.picture().as_deref());
        if self.worker.is_none() {
            let render: worker::RenderFn<P> = P::render;
            self.worker = Some(worker::spawn(&self.ctx.id, self.params.clone(), self.layer.clone(), carrier.clone(), render));
        }
        self.carrier = Some(carrier);
        Ok(ends)
    }

    fn stop(&mut self) -> Result<()> {
        if let Some(w) = self.worker.take() {
            let _ = w.send(Msg::Stop);
        }
        Ok(())
    }

    /// Always in place: the worker renders the new params and the board
    /// draws the new picture from the next frame it has it.
    fn configure(&mut self, params: &Params) -> Result<Configure> {
        let p = P::validate(params)?;
        if let Some(w) = &self.worker {
            let _ = w.send(Msg::Set(p.clone(), p.restarts(&self.params)));
        }
        self.params = p;
        Ok(Configure::Applied)
    }

    fn health(&self) -> Health {
        Health::of(if self.worker.is_some() { PluginState::Running } else { PluginState::Starting })
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value> {
        match method {
            // The pipeline went to NULL and back, which empties the carrier's
            // freeze: give it the picture again.
            "restart" => {
                if let Some(c) = &self.carrier {
                    c.show(self.layer.picture().as_deref());
                }
                Ok(Value::Null)
            }
            other => Err(unknown_method(P::manifest(), other, &["restart"])),
        }
    }
}
