//! Graphic templates: an SVG with named fields in it, drawn by the mixer
//! with no browser.
//!
//! The middle path between a text source (words in a box, almost free) and
//! an OGraf graphic (anything at all, and a Chromium page per graphic). A
//! template is a designed picture, a lower third, a score bug, a title card,
//! whose words and colours are fields:
//!
//! ```text
//!   template.svg --fill {{fields}}--> inline library pictures --shrink to fit--> rsvgdec at the drawn size --> board
//! ```
//!
//! All of it runs on the overlay worker, once per change of a field or of
//! the size the item is drawn at, and never per frame. Between changes the
//! board blends the held picture, and only the part of it with anything in
//! it (`Picture::content`), so a lower third laid out on a whole 1920 by
//! 1080 canvas costs the blend of its panel.

pub mod brand;
pub mod fill;
pub mod fit;
pub mod html;
pub mod images;
pub mod measure;
pub mod pack;
pub mod template;
pub mod xml;

pub use brand::{configure, BrandConfig};
pub use fill::{UnknownField, Values};
pub use template::Template;

use crate::overlay::Picture;
use anyhow::Result;

/// Draw `t` with `values` at `size` pixels.
pub fn render(t: &Template, values: &Values, size: (u32, u32)) -> Result<Picture> {
    let brand = brand::brand();
    let filled = fill::fill(t, values, &brand);
    let library = brand::library();
    let inlined = images::inline(&filled, library.as_deref())?;
    let fitted = fit::shrink(&inlined, &mut |doc, id| measure::width(doc, id))?;
    let picture = crate::plugin::kinds::image_decode::svg_data(&fitted, size)?;
    Ok(Picture { natural: (t.info.width, t.info.height), ..picture }.measured())
}

#[cfg(test)]
mod tests;
