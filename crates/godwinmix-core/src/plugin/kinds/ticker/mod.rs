//! `ticker/source`: words that crawl across a bar, or roll up for credits.
//!
//! The strip of words is rendered once, the bar under it once, and the board
//! moves the strip through the bar each frame by drawing it at a new offset:
//! no glyph is drawn twice. A list of items becomes one strip with the
//! separator between them, and with `loop` the strip follows itself round so
//! the bar is never empty. Speed, direction and loop change in place without
//! the words starting again; new words start again from the edge.

pub mod params;

use super::rendered::{self, Rendering};
use super::text::{compose, glyphs};
use crate::config::Params;
use crate::overlay::worker::Rendered;
use crate::overlay::Picture;
use crate::plugin::source::Provide;
use crate::plugin::{Capability, CapabilitySet, Manifest, MediaDecl, ProvideKind, StreamMode, Tier, API_LEVEL};
use anyhow::Result;
pub use params::{schema, validate, TickerParams, Way};

pub const MANIFEST: Manifest = Manifest {
    plugin: "ticker",
    id: "source",
    kind: ProvideKind::Source,
    api: API_LEVEL,
    description: "A crawl of words across a bar, or credits rolling up, rendered once and moved each frame",
    uri_schemes: &["ticker:"],
    rank: 200,
    media: MediaDecl { video: StreamMode::Raw, audio: StreamMode::Raw, alpha: true, thumb: true },
    capabilities: CapabilitySet::new().with(Capability::RestartInPlace).with(Capability::Health).with(Capability::Alpha),
    latency_ms: 0,
    tier: Tier::Core,
};

pub const PROVIDE: Provide = Provide { manifest: MANIFEST, claims, make: rendered::make::<TickerParams> };

fn claims(uri: &str) -> Option<u16> {
    uri.trim_start().to_ascii_lowercase().starts_with("ticker:").then_some(MANIFEST.rank)
}

impl Rendering for TickerParams {
    fn manifest() -> &'static Manifest {
        &MANIFEST
    }

    fn validate(params: &Params) -> Result<Self> {
        validate(params)
    }

    fn restarts(&self, old: &Self) -> bool {
        self.strip() != old.strip() || self.look != old.look
    }

    /// The bar at the box size and the strip at the matching letter size: a
    /// crawl scales with the bar's height, credits with its width.
    fn render(&self, drawn: Option<(u32, u32)>) -> Result<Rendered> {
        let line = compose::fit(&self.look, None, 1.0).1;
        let natural = (self.width.max(2), if self.direction == Way::Up { line * 6 } else { line });
        let size = drawn.filter(|d| d.0 > 1 && d.1 > 1).unwrap_or(natural);
        let scale = match self.direction {
            Way::Up => size.0 as f64 / natural.0 as f64,
            _ => size.1 as f64 / natural.1 as f64,
        };
        let wrap = (self.direction == Way::Up).then(|| (size.0 as f64 - 2.0 * self.look.padding * scale).max(8.0) as u32);
        let text = self.strip();
        let g = glyphs::render(&glyphs::Ask { text: &text, look: &self.look, scale, wrap })?;
        let strip = g.map(|g| {
            let rgba = compose::Rgba { data: g.rgba, width: g.width, height: g.height };
            Picture::from_ayuv(compose::to_ayuv(&rgba), rgba.width, rgba.height, (rgba.width, rgba.height))
        });
        let bar = compose::compose(&self.look, None, size, scale);
        let backdrop = Picture::from_ayuv(compose::to_ayuv(&bar), bar.width, bar.height, natural);
        Ok(Rendered { picture: strip, motion: self.motion(), backdrop: Some(backdrop) })
    }
}

#[cfg(test)]
mod tests;
