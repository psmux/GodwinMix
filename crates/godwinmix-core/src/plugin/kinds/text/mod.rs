//! `text/source`: words on screen with no browser.
//!
//! Rendered once per change of the words or the look, by `textoverlay` on a
//! worker thread, into a picture the overlay board holds and draws each frame.
//! An unchanged text costs the board's blend of its box and nothing else: no
//! pipeline is running for it but the one held carrier frame. A change sent
//! with `source.set` is applied in place, through `configure`, and the
//! programme never sees a gap.

pub mod compose;
pub mod glyphs;
pub mod style;

use super::{layer::assemble_layer, BuildCtx};
use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::overlay::carrier::Carrier;
use crate::overlay::worker::{self, Msg, Rendered};
use crate::overlay::{Layer, Motion, Picture};
use crate::plugin::source::{unknown_method, Provide, Source, SourceRequest};
use crate::plugin::{
    Capability, CapabilitySet, Configure, Health, Hello, Manifest, MediaDecl, MediaEnds, PluginState, ProvideKind,
    Ready, StreamMode, Tier, API_LEVEL,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use style::Look;

pub const MANIFEST: Manifest = Manifest {
    plugin: "text",
    id: "source",
    kind: ProvideKind::Source,
    api: API_LEVEL,
    description: "Words on screen, in a box or on their own, rendered once and held with no browser",
    uri_schemes: &["text:"],
    rank: 200,
    media: MediaDecl { video: StreamMode::Raw, audio: StreamMode::Raw, alpha: true, thumb: true },
    capabilities: CapabilitySet::new().with(Capability::RestartInPlace).with(Capability::Health).with(Capability::Alpha),
    latency_ms: 0,
    tier: Tier::Core,
};

pub const PROVIDE: Provide = Provide { manifest: MANIFEST, claims, make: new };

fn claims(uri: &str) -> Option<u16> {
    uri.trim_start().to_ascii_lowercase().starts_with("text:").then_some(MANIFEST.rank)
}

/// The params of `text/source`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct TextParams {
    /// The words. A new line in them is a new line on screen. Left out, the
    /// part of the address after `text:` is used.
    pub text: String,
    /// Width of the box in pixels at the item's own size. Left out, the box
    /// is as wide as the longest line; given, lines wrap to fit inside it.
    pub width: Option<u32>,
    /// Height of the box in pixels. Left out, it is as tall as the lines.
    pub height: Option<u32>,
    #[serde(flatten)]
    pub look: Look,
}

const KEYS: &[&str] = &["text", "width", "height"];

/// Check and read `text/source` params.
pub fn validate(params: &Params) -> Result<TextParams> {
    let mut keys = KEYS.to_vec();
    keys.extend(style::LOOK_KEYS);
    style::known_keys("text/source", params, &keys)?;
    let mut p: TextParams = style::read("text/source", params)?;
    p.look.check("text/source")?;
    if p.text.is_empty() {
        if let Some(rest) = params.get("uri").and_then(|v| v.as_str()).and_then(|u| u.trim_start().get(5..)) {
            p.text = rest.replace("\\n", "\n");
        }
    }
    Ok(p)
}

/// The params schema `protocol.json` lists for this kind.
pub fn schema() -> Value {
    serde_json::to_value(schemars::schema_for!(TextParams)).unwrap_or(Value::Null)
}

fn new(req: SourceRequest<'_>) -> Result<Box<dyn Source>> {
    let params = validate(&req.cfg.effective_params())?;
    Ok(Box::new(TextSource { ctx: req.ctx(), params, layer: Layer::new(true), carrier: None, worker: None }))
}

pub struct TextSource {
    ctx: BuildCtx,
    params: TextParams,
    layer: Arc<Layer>,
    carrier: Option<Arc<Carrier>>,
    worker: Option<Sender<Msg<TextParams>>>,
}

/// Render `p` at `drawn`, or at its own size when it has not been drawn yet.
pub fn render(p: &TextParams, drawn: Option<(u32, u32)>) -> Result<Rendered> {
    let wrap_at = |width: u32, scale: f64| p.width.map(|_| (width as f64 - 2.0 * p.look.padding * scale).max(8.0) as u32);
    let natural_glyphs = glyphs::render(&glyphs::Ask { text: &p.text, look: &p.look, scale: 1.0, wrap: p.width.and_then(|w| wrap_at(w, 1.0)) })?;
    let fit = compose::fit(&p.look, natural_glyphs.as_ref(), 1.0);
    let natural = (p.width.unwrap_or(fit.0), p.height.unwrap_or(fit.1));
    let (size, scale, glyphs) = match drawn {
        Some(d) if d != natural && d.0 > 1 && d.1 > 1 => {
            let scale = d.1 as f64 / natural.1.max(1) as f64;
            let g = glyphs::render(&glyphs::Ask { text: &p.text, look: &p.look, scale, wrap: wrap_at(d.0, scale) })?;
            (d, scale, g)
        }
        _ => (natural, 1.0, natural_glyphs),
    };
    let rgba = compose::compose(&p.look, glyphs.as_ref(), size, scale);
    let picture = Picture::from_ayuv(compose::to_ayuv(&rgba), rgba.width, rgba.height, natural);
    Ok(Rendered { picture: Some(picture), motion: Motion::Still })
}

impl Source for TextSource {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        self.params = validate(&hello.params)?;
        self.ctx.canvas = hello.canvas;
        Ok(Ready { manifest: MANIFEST, latency_ms: 0, capabilities: MANIFEST.capabilities })
    }

    fn start(&mut self, canvas: &CanvasCaps, thumb: bool) -> Result<MediaEnds> {
        self.ctx.canvas = canvas.clone();
        let carrier = Arc::new(Carrier::build(&self.ctx.id, canvas)?);
        let ends = assemble_layer(&self.ctx, thumb, &carrier, &self.layer)?;
        carrier.show(self.layer.picture().as_deref());
        if self.worker.is_none() {
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

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        let p = validate(params)?;
        if let Some(w) = &self.worker {
            let _ = w.send(Msg::Set(p.clone(), false));
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
            other => Err(unknown_method(&MANIFEST, other, &["restart"])),
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
