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

use super::rendered::{self, Rendering};
use crate::config::Params;
use crate::overlay::worker::Rendered;
use crate::overlay::{Motion, Picture};
use crate::plugin::source::Provide;
use crate::plugin::{Capability, CapabilitySet, Manifest, MediaDecl, ProvideKind, StreamMode, Tier, API_LEVEL};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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

pub const PROVIDE: Provide = Provide { manifest: MANIFEST, claims, make: rendered::make::<TextParams> };

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
    style::known_keys("text/source", params, &[KEYS, style::LOOK_KEYS].concat())?;
    let mut p: TextParams = style::read("text/source", params)?;
    p.look.check("text/source")?;
    if p.text.is_empty() {
        p.text = from_address(params, "text:");
    }
    Ok(p)
}

/// The words after `scheme` in the source's address, with `\n` as a new line.
pub fn from_address(params: &Params, scheme: &str) -> String {
    let uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or("").trim_start();
    uri.get(scheme.len()..).filter(|_| uri.to_ascii_lowercase().starts_with(scheme)).unwrap_or("").replace("\\n", "\n")
}

/// The params schema `protocol.json` lists for this kind.
pub fn schema() -> Value {
    serde_json::to_value(schemars::schema_for!(TextParams)).unwrap_or(Value::Null)
}

impl Rendering for TextParams {
    fn manifest() -> &'static Manifest {
        &MANIFEST
    }

    fn validate(params: &Params) -> Result<Self> {
        validate(params)
    }

    /// At `drawn`, the letters scale with the box's height and the box takes
    /// the width it is given, so a wider box is more room, not wider letters.
    fn render(&self, drawn: Option<(u32, u32)>) -> Result<Rendered> {
        let p = self;
        let wrap_at = |width: u32, scale: f64| p.width.map(|_| (width as f64 - 2.0 * p.look.padding * scale).max(8.0) as u32);
        let ask = |scale: f64, wrap: Option<u32>| glyphs::Ask { text: &p.text, look: &p.look, scale, wrap };
        let natural_glyphs = glyphs::render(&ask(1.0, p.width.and_then(|w| wrap_at(w, 1.0))))?;
        let fit = compose::fit(&p.look, natural_glyphs.as_ref(), 1.0);
        let natural = (p.width.unwrap_or(fit.0), p.height.unwrap_or(fit.1));
        let (size, scale, glyphs) = match drawn {
            Some(d) if d != natural && d.0 > 1 && d.1 > 1 => {
                let scale = d.1 as f64 / natural.1.max(1) as f64;
                (d, scale, glyphs::render(&ask(scale, wrap_at(d.0, scale)))?)
            }
            _ => (natural, 1.0, natural_glyphs),
        };
        let rgba = compose::compose(&p.look, glyphs.as_ref(), size, scale);
        let picture = Picture::from_ayuv(compose::to_ayuv(&rgba), rgba.width, rgba.height, natural);
        Ok(Rendered { picture: Some(picture), motion: Motion::Still, backdrop: None })
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
