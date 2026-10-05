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
pub mod opaque;
pub mod page;
pub mod params;
pub mod renderer;
pub mod wire;
mod source;

/// What `source` needs from the kinds module.
mod layer_parts {
    pub use super::super::layer::assemble_layer;
    pub use super::super::BuildCtx;
}

use crate::overlay::Layer;
use crate::plugin::source::{Provide, Source, SourceRequest};
use crate::plugin::{Capability, CapabilitySet, Manifest, MediaDecl, ProvideKind, StreamMode, Tier, API_LEVEL};
use anyhow::Result;
use source::HtmlSource;

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
    Ok(Box::new(HtmlSource::new(req.ctx(), params, Layer::new(true))))
}

#[cfg(test)]
mod tests;
