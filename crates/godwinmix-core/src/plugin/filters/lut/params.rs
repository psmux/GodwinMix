//! What `lut/filter` takes, and how a preset fills in what is not given.
//!
//! | param | what | default |
//! |---|---|---|
//! | `lut` | a LUT in the library by name: built in or imported | none |
//! | `file` | a LUT file on the mixer's machine, read directly | none |
//! | `preset` | a saved preset, whose values fill what is not given | none |
//! | `amount` | 0 is none of the LUT, 1 is all of it | 1 |
//! | `brightness`, `contrast`, `gamma`, `saturation`, `hue_shift` | the colour correction, as `color/filter` takes it, run before the LUT | 0 |
//!
//! `file` is what an OBS "Apply LUT" filter becomes, since OBS names the
//! file it reads. `clut_amount` is read as `amount` for the same reason.

use crate::config::Params;
use crate::lut::library::{self, Preset};
use crate::lut::Lut;
use crate::plugin::filters::audio::values::num;
use crate::plugin::filters::colour::Settings as Colour;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const KIND: &str = "lut/filter";

/// The params with a preset's values laid under them.
pub fn merged(params: &Params, dir: &Path) -> Result<Params> {
    let Some(name) = params.get("preset").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) else {
        return Ok(params.clone());
    };
    let p = library::preset(dir, name)?;
    let mut out = preset_params(&p);
    for (k, v) in params {
        out.insert(k.clone(), v.clone());
    }
    Ok(out)
}

/// A preset as the params it stands for.
pub fn preset_params(p: &Preset) -> Params {
    let mut out = Params::new();
    if let Some(l) = &p.lut {
        out.insert("lut".into(), l.clone().into());
    }
    let nums = [("amount", p.amount), ("brightness", p.brightness), ("contrast", p.contrast), ("gamma", p.gamma), ("saturation", p.saturation), ("hue_shift", p.hue_shift)];
    for (k, v) in nums {
        if let Some(v) = v {
            out.insert(k.into(), v.into());
        }
    }
    out
}

pub fn amount(params: &Params) -> Result<f32> {
    Ok(num(KIND, params, &["amount", "clut_amount"], 1.0, 0.0, 1.0)? as f32)
}

pub fn colour(params: &Params) -> Result<Colour> {
    Colour::from_params(params)
}

/// Which LUT the params name, if any.
pub enum Source {
    None,
    Named(String),
    File(PathBuf),
}

pub fn source(params: &Params) -> Source {
    let text = |k: &str| params.get(k).and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty());
    match (text("file").or_else(|| text("image_path")), text("lut")) {
        (Some(f), _) => Source::File(PathBuf::from(f)),
        (None, Some(n)) => Source::Named(n.to_string()),
        _ => Source::None,
    }
}

pub fn load(source: &Source, dir: &Path) -> Result<Option<Arc<Lut>>> {
    match source {
        Source::None => Ok(None),
        Source::Named(n) => library::load(dir, n).map(Some),
        Source::File(p) => library::load_file(p)
            .with_context(|| format!("the LUT file {} would not read. Import it with lut.import so the library keeps a copy", p.display()))
            .map(Some),
    }
}

/// Ranges only: a LUT that is missing is not a reason to refuse a config,
/// since the filter passes the picture through until it is there.
pub fn validate(params: &Params) -> Result<()> {
    amount(params)?;
    colour(params)?;
    Ok(())
}
