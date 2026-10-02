//! What a text looks like: the params `text/source` and `ticker/source`
//! share, read from a source's `params`, with the error a caller can act on.

use crate::config::Params;
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// How lines sit against each other and against the box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// Where the words sit up and down in a box taller than they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Valign {
    Top,
    #[default]
    Middle,
    Bottom,
}

/// The look of the words, shared by text and ticker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Look {
    /// A font family installed on the mixer, such as `Sans`, `Serif` or
    /// `DejaVu Sans`. A name the machine does not have falls back to its
    /// default sans serif.
    pub font: String,
    /// Height of the letters in canvas pixels, at the item's own size.
    pub size: f64,
    /// 100 (thin) to 900 (black). 400 is regular, 700 bold.
    pub weight: u16,
    pub italic: bool,
    /// The letters' colour: `#rgb`, `#rrggbb` or `#rrggbbaa`.
    pub color: String,
    /// A colour for an outline round each letter, or empty for none.
    pub outline: String,
    /// A soft dark shadow under the letters.
    pub shadow: bool,
    /// A box behind the words: a colour with optional alpha
    /// (`#000000b3`), or empty for no box.
    pub background: String,
    /// Space between the words and the edge of the box, in pixels.
    pub padding: f64,
    /// Corner radius of the box, in pixels.
    pub radius: f64,
    pub align: Align,
    pub valign: Valign,
}

impl Default for Look {
    fn default() -> Self {
        Look {
            font: "Sans".into(),
            size: 48.0,
            weight: 600,
            italic: false,
            color: "#ffffff".into(),
            outline: String::new(),
            shadow: false,
            background: "#000000b3".into(),
            padding: 24.0,
            radius: 12.0,
            align: Align::Left,
            valign: Valign::Middle,
        }
    }
}

impl Look {
    /// Every field checked, so a bad colour is refused when it is set rather
    /// than drawn as nothing.
    pub fn check(&self, kind: &str) -> Result<()> {
        anyhow::ensure!(
            self.size > 0.0 && self.size <= 1000.0,
            "{kind} params.size is the letter height in pixels, more than 0 and at most 1000; got {}",
            self.size
        );
        anyhow::ensure!(
            (100..=900).contains(&self.weight),
            "{kind} params.weight is 100 to 900 (400 regular, 700 bold); got {}",
            self.weight
        );
        anyhow::ensure!(self.padding >= 0.0 && self.radius >= 0.0, "{kind} params.padding and params.radius cannot be negative");
        for (name, value, empty_ok) in [("color", &self.color, false), ("outline", &self.outline, true), ("background", &self.background, true)] {
            if !(empty_ok && value.trim().is_empty()) {
                colour(value).map_err(|e| anyhow::anyhow!("{kind} params.{name}: {e}"))?;
            }
        }
        Ok(())
    }

    /// The Pango description `textoverlay` is given, at `scale`.
    pub fn font_desc(&self, scale: f64) -> String {
        let weight = match self.weight {
            0..=149 => "Thin",
            150..=249 => "Ultra-Light",
            250..=349 => "Light",
            350..=449 => "",
            450..=549 => "Medium",
            550..=649 => "Semi-Bold",
            650..=749 => "Bold",
            750..=849 => "Ultra-Bold",
            _ => "Heavy",
        };
        let italic = if self.italic { "Italic" } else { "" };
        let px = (self.size * scale).max(1.0).round();
        let family = self.font.replace(',', " ");
        format!("{}, {weight} {italic} {px}px", family.trim())
    }
}

/// `#rgb`, `#rrggbb` or `#rrggbbaa` as straight RGBA bytes.
pub fn colour(s: &str) -> Result<[u8; 4]> {
    let hex = s.trim().trim_start_matches('#');
    let digits: Vec<u8> = hex.chars().map(|c| c.to_digit(16).map(|d| d as u8)).collect::<Option<_>>().ok_or_else(|| bad(s))?;
    let pair = |i: usize| digits[i] * 16 + digits[i + 1];
    Ok(match digits.len() {
        3 => [digits[0] * 17, digits[1] * 17, digits[2] * 17, 255],
        6 => [pair(0), pair(2), pair(4), 255],
        8 => [pair(0), pair(2), pair(4), pair(6)],
        _ => return Err(bad(s)),
    })
}

fn bad(s: &str) -> anyhow::Error {
    anyhow::anyhow!("{s:?} is not a colour. Write #rgb, #rrggbb, or #rrggbbaa with the last two digits the opacity")
}

/// Refuse a key the kind does not take, naming the ones it does.
pub fn known_keys(kind: &str, params: &Params, keys: &[&str]) -> Result<()> {
    for key in params.keys() {
        if key != "uri" && !keys.contains(&key.as_str()) {
            anyhow::bail!("{kind} has no param {key:?}. It takes: {}", keys.join(", "));
        }
    }
    Ok(())
}

/// The param names `Look` has, for `known_keys`.
pub const LOOK_KEYS: &[&str] = &[
    "font", "size", "weight", "italic", "color", "outline", "shadow", "background", "padding", "radius", "align", "valign",
];

/// Read `T` out of a source's params, with the field named in the error.
pub fn read<T: serde::de::DeserializeOwned>(kind: &str, params: &Params) -> Result<T> {
    let mut value = serde_json::to_value(params)?;
    if let Some(map) = value.as_object_mut() {
        map.remove("uri");
    }
    serde_json::from_value(value).map_err(|e| anyhow::anyhow!("{kind} params: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_read_the_three_ways_and_say_what_they_want_otherwise() {
        assert_eq!(colour("#fff").unwrap(), [255, 255, 255, 255]);
        assert_eq!(colour("#1f6f4f").unwrap(), [0x1f, 0x6f, 0x4f, 255]);
        assert_eq!(colour("#000000b3").unwrap(), [0, 0, 0, 0xb3]);
        let e = colour("red").unwrap_err().to_string();
        assert!(e.contains("#rrggbb"), "{e}");
    }

    #[test]
    fn the_font_description_scales_and_names_the_weight() {
        let look = Look { font: "DejaVu Sans".into(), size: 40.0, weight: 700, ..Look::default() };
        assert_eq!(look.font_desc(1.5).split_whitespace().collect::<Vec<_>>(), ["DejaVu", "Sans,", "Bold", "60px"]);
    }
}
