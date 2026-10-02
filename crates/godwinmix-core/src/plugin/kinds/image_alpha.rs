//! `image/source` for a picture with transparency: a PNG or WebP with an
//! alpha channel, or any SVG.
//!
//! Decoded once into the AYUV the overlay board draws, and held. Drawn at a
//! size other than its own, a raster picture is scaled once to that size and
//! an SVG is drawn again at it by `rsvgdec`, so neither is scaled every frame
//! and an SVG stays sharp at any size. Nothing runs between changes.

use super::image_decode;
use super::image_probe::{has_alpha, svg_size};
use super::rendered::Rendering;
use crate::config::Params;
use crate::overlay::worker::Rendered;
use crate::overlay::{Motion, Picture};
use crate::plugin::{Capability, CapabilitySet, Manifest, MediaDecl, StreamMode};
use anyhow::{Context, Result};
use gstreamer as gst;
use std::sync::{Arc, Mutex};

pub const MANIFEST: Manifest = Manifest {
    media: MediaDecl { video: StreamMode::Container, audio: StreamMode::Raw, alpha: true, thumb: true },
    capabilities: CapabilitySet::new().with(Capability::RestartInPlace).with(Capability::Health).with(Capability::Alpha),
    ..super::image::MANIFEST
};

/// The largest an SVG is drawn before anything has placed it. Its own size
/// can say anything, and a 10000 pixel square drawn for nobody is not free.
const FIRST_DRAW_MAX: (u32, u32) = (1920, 1080);

/// One transparent picture, and the decode of it at its own size once made.
#[derive(Clone)]
pub struct AlphaStill {
    uri: String,
    svg: Option<(u32, u32)>,
    held: Arc<Mutex<Option<Arc<Picture>>>>,
}

/// Whether the picture at `uri` should go to the overlay board, by
/// `params.alpha` (`auto`, `true`, `false`) and then by the file itself.
/// A picture somewhere this cannot look before decoding is taken as opaque
/// unless it is an SVG or the params say otherwise.
pub fn wanted(uri: &str, params: &Params) -> Result<bool> {
    match params.get("alpha") {
        Some(toml::Value::Boolean(b)) => return Ok(*b),
        Some(toml::Value::String(s)) if s == "auto" => {}
        None => {}
        Some(other) => anyhow::bail!("image/source params.alpha is true, false or \"auto\"; got {other}"),
    }
    if uri.to_ascii_lowercase().split(['?', '#']).next().unwrap_or("").ends_with(".svg") {
        return Ok(true);
    }
    let path = gst::glib::filename_from_uri(uri).map(|(p, _)| p).unwrap_or_else(|_| uri.into());
    Ok(has_alpha(&path).unwrap_or(false))
}

impl PartialEq for AlphaStill {
    fn eq(&self, other: &Self) -> bool {
        self.uri == other.uri
    }
}

impl Rendering for AlphaStill {
    fn manifest() -> &'static Manifest {
        &MANIFEST
    }

    fn validate(params: &Params) -> Result<Self> {
        let raw = params.get("uri").and_then(|v| v.as_str()).context("image/source needs the picture's address in uri")?;
        let uri = crate::input::to_uri(raw);
        let svg = uri.to_ascii_lowercase().ends_with(".svg").then(|| svg_declared(&uri));
        Ok(AlphaStill { uri, svg, held: Arc::new(Mutex::new(None)) })
    }

    fn render(&self, drawn: Option<(u32, u32)>) -> Result<Rendered> {
        let picture = match self.svg {
            Some(natural) => {
                let size = drawn.unwrap_or_else(|| contain(natural, FIRST_DRAW_MAX));
                Picture { natural, ..image_decode::svg(&self.uri, size)? }
            }
            None => {
                let own = self.own()?;
                match drawn.filter(|d| *d != (own.width, own.height)) {
                    Some(d) => image_decode::scaled(&own, d)?,
                    None => Picture { buffer: own.buffer.clone(), keyed: own.keyed.clone(), ..*own },
                }
            }
        };
        Ok(Rendered { picture: Some(picture), motion: Motion::Still, backdrop: None })
    }
}

impl AlphaStill {
    /// The picture at its own size, decoded the first time it is wanted.
    fn own(&self) -> Result<Arc<Picture>> {
        let mut held = self.held.lock().map_err(|_| anyhow::anyhow!("picture lock poisoned"))?;
        if let Some(p) = held.as_ref() {
            return Ok(p.clone());
        }
        let p = Arc::new(image_decode::raster(&self.uri)?);
        *held = Some(p.clone());
        Ok(p)
    }
}

/// The size the SVG says it is, or a square when it says nothing usable.
fn svg_declared(uri: &str) -> (u32, u32) {
    let path = gst::glib::filename_from_uri(uri).map(|(p, _)| p).ok();
    let text = path.and_then(|p| {
        use std::io::Read;
        let mut head = Vec::new();
        std::fs::File::open(p).ok()?.take(16 * 1024).read_to_end(&mut head).ok()?;
        Some(String::from_utf8_lossy(&head).to_string())
    });
    text.as_deref().and_then(svg_size).unwrap_or((512, 512))
}

/// `size` shrunk to fit inside `max`, keeping its shape.
fn contain(size: (u32, u32), max: (u32, u32)) -> (u32, u32) {
    let s = (max.0 as f64 / size.0.max(1) as f64).min(max.1 as f64 / size.1.max(1) as f64).min(1.0);
    (((size.0 as f64 * s).round() as u32).max(1), ((size.1 as f64 * s).round() as u32).max(1))
}

#[cfg(test)]
#[path = "image_alpha_tests.rs"]
mod tests;
