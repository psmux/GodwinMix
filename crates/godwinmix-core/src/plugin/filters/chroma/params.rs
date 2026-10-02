//! What the key takes, read from the filter's params and checked.
//!
//! Every setting has a default, so `{}` is a working key: the colour is
//! guessed from the picture, and the rest are numbers that suit a lit green
//! screen. A bad value names the field and its range.
//!
//! Two older spellings are read as well. The settings OBS writes
//! (`similarity` and `smoothness` from 1 to 1000, `key_color_type`,
//! `key_color` as a packed integer) come across with an OBS import, and the
//! `alpha` element's `angle`, `noise` and `spread` are in documents written
//! before this key replaced it. Those three are accepted and ignored.

use crate::config::Params;
use super::colour::parse_hex;
use anyhow::{bail, Result};

/// Which screen colour to look for when the colour is guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Green,
    Blue,
    /// Any saturated colour: what `custom` means with no colour given.
    Any,
}

/// The key colour: a fixed one, or a guess from the first frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Colour {
    Auto,
    Rgb([u8; 3]),
}

/// The area kept, as fractions of the picture cut off each edge.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Matte {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub colour: Colour,
    pub family: Family,
    /// How far from the key colour a pixel can be and still go fully clear.
    pub similarity: f32,
    /// How wide the band between clear and solid is: the edge softness.
    pub smoothness: f32,
    /// How much of the key colour is taken out of what stays.
    pub spill: f32,
    /// The edge of the matte blurred over this many pixels.
    pub feather: u32,
    pub matte: Matte,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            colour: Colour::Auto,
            family: Family::Green,
            similarity: 0.40,
            smoothness: 0.10,
            spill: 0.60,
            feather: 1,
            matte: Matte::default(),
        }
    }
}

/// Every key a person can set, for the error that lists them.
pub const KEYS: &str = "color, method, similarity, smoothness, spill, feather, matte_left, \
                        matte_top, matte_right, matte_bottom";

impl Settings {
    /// Read and check params. Anything absent keeps its default.
    pub fn from_params(params: &Params) -> Result<Settings> {
        let mut s = Settings::default();
        let mut rgb: [Option<u8>; 3] = [None; 3];
        for (key, value) in params {
            match key.as_str() {
                "color" | "colour" | "key" => s.colour = colour(key, value)?,
                "method" | "key_color_type" => s.family = family(key, value)?,
                "key_color" => s.colour = Colour::Rgb(obs_colour(value)?),
                "target_r" => rgb[0] = Some(byte(key, value)?),
                "target_g" => rgb[1] = Some(byte(key, value)?),
                "target_b" => rgb[2] = Some(byte(key, value)?),
                "similarity" => s.similarity = fraction(key, value, true)?,
                "smoothness" => s.smoothness = fraction(key, value, true)?.max(0.005),
                "spill" => s.spill = fraction(key, value, true)?,
                "feather" => s.feather = whole(key, value, 0, 20)?,
                "matte_left" => s.matte.left = fraction(key, value, false)?,
                "matte_top" => s.matte.top = fraction(key, value, false)?,
                "matte_right" => s.matte.right = fraction(key, value, false)?,
                "matte_bottom" => s.matte.bottom = fraction(key, value, false)?,
                "angle" | "noise" | "spread" | "id" | "opacity" | "contrast" | "brightness"
                | "gamma" => {}
                other => bail!("chroma/filter has no setting `{other}`. It takes: {KEYS}"),
            }
        }
        if let [Some(r), Some(g), Some(b)] = rgb {
            s.colour = Colour::Rgb([r, g, b]);
        }
        let m = s.matte;
        if m.left + m.right >= 0.98 || m.top + m.bottom >= 0.98 {
            bail!(
                "chroma/filter's matte leaves nothing: left {} and right {} (or top {} and \
                 bottom {}) add up to the whole picture. Keep each pair under 0.98.",
                m.left, m.right, m.top, m.bottom
            );
        }
        Ok(s)
    }
}

fn colour(key: &str, value: &toml::Value) -> Result<Colour> {
    let text = value.as_str().unwrap_or_default().trim();
    if text.is_empty() || text.eq_ignore_ascii_case("auto") {
        return Ok(Colour::Auto);
    }
    parse_hex(text).map(Colour::Rgb).ok_or_else(|| {
        anyhow::anyhow!(
            "chroma/filter params.{key} must be \"auto\" or a colour like \"#30b050\", not `{value}`"
        )
    })
}

fn family(key: &str, value: &toml::Value) -> Result<Family> {
    match value.as_str().unwrap_or_default() {
        "green" => Ok(Family::Green),
        "blue" => Ok(Family::Blue),
        "custom" | "magenta" => Ok(Family::Any),
        other => bail!("chroma/filter params.{key} must be green, blue or custom, not `{other}`"),
    }
}

/// OBS packs the colour as 0xAABBGGRR in an integer.
fn obs_colour(value: &toml::Value) -> Result<[u8; 3]> {
    let n = value.as_integer().filter(|n| *n >= 0).ok_or_else(|| {
        anyhow::anyhow!("chroma/filter params.key_color must be OBS's packed colour, not `{value}`")
    })?;
    Ok([(n & 0xff) as u8, ((n >> 8) & 0xff) as u8, ((n >> 16) & 0xff) as u8])
}

fn number(value: &toml::Value) -> Option<f64> {
    match value {
        toml::Value::Float(f) => Some(*f),
        toml::Value::Integer(i) => Some(*i as f64),
        _ => None,
    }
}

/// 0 to 1. Where `obs` is set, a number from 1 to 1000 is read as OBS's own
/// scale and divided by 1000, so an imported key keeps its look.
fn fraction(key: &str, value: &toml::Value, obs: bool) -> Result<f32> {
    let n = number(value).unwrap_or(-1.0);
    let n = if obs && n > 1.0 && n <= 1000.0 { n / 1000.0 } else { n };
    if !(0.0..=1.0).contains(&n) {
        bail!("chroma/filter params.{key} must be a number from 0 to 1, not `{value}`");
    }
    Ok(n as f32)
}

fn byte(key: &str, value: &toml::Value) -> Result<u8> {
    Ok(whole(key, value, 0, 255)? as u8)
}

fn whole(key: &str, value: &toml::Value, lo: u32, hi: u32) -> Result<u32> {
    match number(value) {
        Some(n) if n >= lo as f64 && n <= hi as f64 => Ok(n.round() as u32),
        _ => bail!("chroma/filter params.{key} must be a whole number from {lo} to {hi}, not `{value}`"),
    }
}
