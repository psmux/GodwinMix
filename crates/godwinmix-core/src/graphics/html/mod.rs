//! HTML templates: a designed graphic that moves.
//!
//! The same idea as an SVG template (a design with named fields, found by
//! name, its fields set with `source.set`), written as a web page so it can
//! do what a still cannot: CSS animation, a canvas, WebGL, a way in and a
//! way out of its own. It is drawn by the browser renderer in graphic mode
//! (`html/graphic`), which sends only the part of the page that has
//! something in it and only when it changed, so a graphic that holds still
//! costs next to nothing and one that moves costs what its moving part does.
//!
//! The page declares itself in one JSON block (`meta`), shows a field with
//! `data-field="name"` or `var(--name)`, and moves in and out under the
//! `gmx-in` and `gmx-out` classes the renderer puts on `<html>`. The pack is
//! in `graphics/html/`; a station's own are `.html` files in its media
//! library. See `docs/reference/graphics-for-agents.md`.

pub mod check;
pub mod fields;
pub mod library;
pub mod meta;
pub mod pack;
pub mod problem;

use crate::graphics::template::{Template, TemplateFormat, TemplateInfo, TemplateOrigin};
use anyhow::{bail, Result};
use std::path::PathBuf;

/// The address scheme an HTML template is added by.
pub const SCHEME: &str = "html:";

/// An HTML template, read and checked.
#[derive(Debug, Clone, PartialEq)]
pub struct HtmlTemplate {
    pub info: TemplateInfo,
    pub html: String,
    /// The file the renderer loads: a library file, or the pack's copy on
    /// disk (`pack::page`). None until it is known.
    pub file: Option<PathBuf>,
    /// The frame rate the design asks for, when it asks.
    pub fps: Option<u32>,
    /// Drawn at this share of the canvas size and stretched: 1 unless the
    /// design asks for less.
    pub resolution: f64,
}

impl HtmlTemplate {
    /// Read `html` as the template `name`, refusing one with an error in it.
    pub fn parse(name: &str, origin: TemplateOrigin, html: String) -> Result<HtmlTemplate> {
        let (problems, meta) = check::check(&html);
        if !problem::ok(&problems) {
            bail!("{}", problem::message(name, &problems));
        }
        let meta = meta.unwrap_or_default();
        let (fps, resolution) = (meta.fps, meta.resolution.unwrap_or(1.0));
        let stem = name.trim_end_matches(".html");
        let info = TemplateInfo {
            name: name.into(),
            title: meta.title.unwrap_or_else(|| stem.replace(['-', '_'], " ")),
            description: meta.description,
            origin,
            uri: format!("{SCHEME}{name}"),
            width: 1920,
            height: 1080,
            fields: meta.fields,
            format: TemplateFormat::Html,
            category: meta.category,
            out_ms: meta.out_ms,
            opaque: meta.opaque,
        };
        Ok(HtmlTemplate { info, html, file: None, fps, resolution })
    }

    /// The same template as the SVG checks see it, for `fill::check`.
    pub fn as_template(&self) -> Template {
        Template { info: self.info.clone(), svg: String::new() }
    }
}

/// The template name in an `html:` address.
pub fn name_in(uri: &str) -> Option<&str> {
    let uri = uri.trim();
    uri.get(..SCHEME.len()).filter(|s| s.eq_ignore_ascii_case(SCHEME)).map(|_| uri[SCHEME.len()..].trim())
}

#[cfg(test)]
#[path = "pack_tests.rs"]
mod pack_tests;
