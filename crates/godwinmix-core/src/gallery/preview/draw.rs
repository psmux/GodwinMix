//! The layers of one preview, each drawn by the code that draws the item
//! on air, at the box its zone gives it on a canvas the preview's size.

use super::clip;
use super::rgba::{from_picture, Layer};
use crate::gallery::{detect, entry::Entry, entry::Where, place};
use crate::overlay::Picture;
use crate::plugin::kinds::rendered::Rendering;
use anyhow::{Context, Result};
use godwinmix_protocol::gallery::{GalleryKind, Zone};
use serde_json::{Map, Value};
use std::path::Path;

type Size = (u32, u32);

/// The layers, bottom first, and how they were made.
pub fn layers(gallery: &Path, entry: &Entry, canvas: Size, values: &Map<String, Value>) -> Result<(Vec<Layer>, &'static str)> {
    let zone = entry.item.zone;
    let file = entry.file();
    let drawn = |l: Layer| Ok((vec![l], "drawn"));
    match entry.item.kind {
        GalleryKind::Template => drawn(template(entry, canvas, values)?),
        GalleryKind::Image => drawn(picture(file.as_deref().context("the item has no picture file")?, zone, canvas)?),
        GalleryKind::Clip => Ok((vec![clip::frame(file.as_deref().context("the item has no clip file")?, zone, canvas)?], "frame")),
        GalleryKind::Ticker | GalleryKind::Text => drawn(words(entry, zone, canvas)?),
        GalleryKind::Set => Ok((set(gallery, entry, canvas)?, "drawn")),
        _ => other(entry, canvas),
    }
}

/// A page, an OGraf graphic, a transition or an effect: its own preview
/// file, else a frame of its clip, else a card that names it.
fn other(entry: &Entry, canvas: Size) -> Result<(Vec<Layer>, &'static str)> {
    if let Some(poster) = entry.preview_file() {
        return Ok((vec![picture(&poster, Zone::Full, canvas)?], "poster"));
    }
    if let Some(file) = entry.file().filter(|f| f.is_file()) {
        let ext = file.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if matches!(ext.as_str(), "webm" | "mov" | "mp4" | "mkv") {
            return Ok((vec![clip::frame(&file, Zone::Full, canvas)?], "frame"));
        }
        if matches!(ext.as_str(), "png" | "webp" | "jpg" | "jpeg" | "svg") {
            return Ok((vec![picture(&file, Zone::Full, canvas)?], "poster"));
        }
    }
    Ok((card(entry, canvas).into_iter().collect(), "card"))
}

/// The box `zone` gives an item of `natural` size, as a position and a size.
fn boxed(zone: Zone, canvas: Size, natural: Option<Size>) -> ((i32, i32), Size) {
    let t = place::transform(zone, canvas, natural);
    let n = |p: &str| t.pointer(p).and_then(Value::as_f64).unwrap_or(0.0);
    ((n("/position/x") as i32, n("/position/y") as i32), ((n("/frame/w") as u32).max(2), (n("/frame/h") as u32).max(2)))
}

/// Fit `natural` inside `size`, keeping its shape, centred.
fn contain(at: (i32, i32), size: Size, natural: Size) -> ((i32, i32), Size) {
    let s = (size.0 as f64 / natural.0.max(1) as f64).min(size.1 as f64 / natural.1.max(1) as f64);
    let (w, h) = (((natural.0 as f64 * s) as u32).max(2), ((natural.1 as f64 * s) as u32).max(2));
    ((at.0 + (size.0 as i32 - w as i32) / 2, at.1 + (size.1 as i32 - h as i32) / 2), (w, h))
}

fn template(entry: &Entry, canvas: Size, values: &Map<String, Value>) -> Result<Layer> {
    let t = match &entry.at {
        Where::Pack(name) => crate::graphics::pack::load(name)?,
        _ => crate::graphics::pack::load(&entry.file().context("the template's file")?.display().to_string())?,
    };
    let mut merged = entry.item.values.clone();
    merged.extend(values.clone());
    let vals: crate::graphics::Values = merged
        .iter()
        .filter(|(_, v)| !v.is_null())
        .map(|(k, v)| (k.clone(), v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())))
        .collect();
    crate::graphics::fill::check(&t, &vals).map_err(|e| anyhow::anyhow!("{e}"))?;
    let (at, size) = boxed(entry.item.zone, canvas, Some((t.info.width, t.info.height)));
    let (at, size) = contain(at, size, (t.info.width, t.info.height));
    from_picture(&crate::graphics::render(&t, &vals, size)?, size, at)
}

/// A still picture file in its zone: an SVG drawn at the size, anything
/// else decoded once and scaled.
fn picture(file: &Path, zone: Zone, canvas: Size) -> Result<Layer> {
    let uri = crate::input::to_uri(&file.display().to_string());
    let svg = file.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg"));
    let natural = if svg { detect::size(file) } else { None };
    let pic: Picture = match natural {
        Some(n) => {
            let (at, size) = boxed(zone, canvas, Some(n));
            let (at, size) = contain(at, size, n);
            return from_picture(&crate::plugin::kinds::image_decode::svg(&uri, size)?, size, at);
        }
        None => crate::plugin::kinds::image_decode::raster(&uri)?,
    };
    let own = (pic.width, pic.height);
    let (at, size) = boxed(zone, canvas, Some(own));
    let (at, size) = contain(at, size, own);
    from_picture(&pic, size, at)
}

/// A ticker or a text, drawn by its own kind at its box.
fn words(entry: &Entry, zone: Zone, canvas: Size) -> Result<Layer> {
    let spec = entry.manifest.source.as_ref().context("the item has no [source]")?;
    let mut params = spec.params.clone();
    params.insert("uri".into(), toml::Value::String(spec.uri.clone()));
    let (at, size) = boxed(zone, canvas, None);
    let rendered = if entry.item.kind == GalleryKind::Ticker {
        crate::plugin::kinds::ticker::TickerParams::validate(&params)?.render(Some(size))?
    } else {
        crate::plugin::kinds::text::TextParams::validate(&params)?.render(Some(size))?
    };
    // At their own sizes: a crawl's strip is wider than its bar, and the
    // preview shows its start, cut at the bar's edge as on air.
    let mut img = Vec::new();
    for pic in rendered.backdrop.iter().chain(rendered.picture.iter()) {
        img.push(from_picture(pic, (pic.width.min(8192), pic.height), at)?);
    }
    flatten(img, at, size)
}

/// Several layers at one place, as one. The first is the bottom.
fn flatten(layers: Vec<Layer>, at: (i32, i32), size: Size) -> Result<Layer> {
    let mut img = image::RgbaImage::new(size.0, size.1);
    for l in &layers {
        for y in 0..l.height.min(size.1) {
            for x in 0..l.width.min(size.0) {
                let i = ((y * l.width + x) * 4) as usize;
                let src = &l.rgba[i..i + 4];
                let px = img.get_pixel_mut(x, y);
                let a = src[3] as u32;
                for c in 0..3 {
                    px.0[c] = ((src[c] as u32 * a + px.0[c] as u32 * (255 - a)) / 255) as u8;
                }
                px.0[3] = px.0[3].max(src[3]);
            }
        }
    }
    Ok(Layer { rgba: img.into_raw(), width: size.0, height: size.1, x: at.0, y: at.1 })
}

/// A set: its background over the canvas and its foreground over that.
fn set(_gallery: &Path, entry: &Entry, canvas: Size) -> Result<Vec<Layer>> {
    let spec = entry.manifest.set.as_ref().context("the set has no [set]")?;
    let dir = entry.dir().context("the set's folder")?;
    let mut out = vec![picture(&dir.join(&spec.background), Zone::Full, canvas)?];
    if let Some(front) = spec.foreground.as_ref().filter(|f| dir.join(f).is_file()) {
        out.push(picture(&dir.join(front), Zone::Overlay, canvas)?);
    }
    Ok(out)
}

/// A card naming the item, drawn by the text kind.
fn card(entry: &Entry, canvas: Size) -> Option<Layer> {
    let text = format!("{}\n{}", entry.item.kind.as_str().to_uppercase(), entry.item.name);
    let mut params = toml::Table::new();
    params.insert("uri".into(), "text:".into());
    params.insert("text".into(), text.into());
    params.insert("color".into(), "#ffffff".into());
    params.insert("background".into(), "#1d2433".into());
    params.insert("size".into(), toml::Value::Float(40.0));
    params.insert("padding".into(), toml::Value::Float(28.0));
    let size = (canvas.0 * 3 / 4, canvas.1 / 3);
    let at = (((canvas.0 - size.0) / 2) as i32, ((canvas.1 - size.1) / 2) as i32);
    let pic = crate::plugin::kinds::text::TextParams::validate(&params).ok()?.render(Some(size)).ok()?.picture?;
    from_picture(&pic, size, at).ok()
}
