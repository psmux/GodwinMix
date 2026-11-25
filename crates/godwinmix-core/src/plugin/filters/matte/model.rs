//! Which model file cuts the person out, and how to feed it.
//!
//! Models are data, not code. Two ship with GodwinMix, both Apache 2.0:
//!
//! | name | file | input | what it answers | cost |
//! |---|---|---|---|---|
//! | fast | `selfie.onnx` | 256 x 256, 0 to 1 | person or not, softly | ~6 ms on a laptop CPU |
//! | fine | `modnet.onnx` | 512 x 288, -1 to 1 | an alpha matte | ~30 ms on a laptop GPU |
//!
//! Any other model that takes one picture and answers one matte can be added
//! by dropping `name.onnx` in a models folder with `name.json` beside it:
//!
//! ```json
//! {"width": 512, "height": 288, "mean": 0.5, "std": 0.5}
//! ```
//!
//! The picture goes in as 1 x 3 x height x width, RGB, `(value / 255 - mean)
//! / std`; the first output is read as 1 x 1 x h x w, 0 to 1.

use super::params::{Device, Quality};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    pub name: String,
    pub file: PathBuf,
    pub width: usize,
    pub height: usize,
    pub mean: f32,
    pub std: f32,
}

/// The two that ship: name, file, width, height, mean, std.
const BUILT_IN: [(&str, &str, usize, usize, f32, f32); 2] = [
    ("fast", "selfie.onnx", 256, 256, 0.0, 1.0),
    ("fine", "modnet.onnx", 512, 288, 0.5, 0.5),
];

/// The model a setting asks for, found on disk. `gpu` says whether a GPU will
/// run it, which is what `auto` decides on.
pub fn choose(quality: &Quality, gpu: bool) -> Result<Spec> {
    let name = match quality {
        Quality::Auto if gpu => "fine",
        Quality::Auto | Quality::Fast => "fast",
        Quality::Fine => "fine",
        Quality::Named(n) => return named(n),
    };
    let (n, file, width, height, mean, std) = BUILT_IN.iter().copied().find(|b| b.0 == name).expect("a built in");
    let found = find(file).with_context(|| missing(file))?;
    Ok(Spec { name: n.into(), file: found, width, height, mean, std })
}

/// Whether a device setting may use a GPU at all.
pub fn wants_gpu(device: Device) -> bool {
    device != Device::Cpu
}

fn named(name: &str) -> Result<Spec> {
    let file = find(&format!("{name}.onnx")).with_context(|| missing(&format!("{name}.onnx")))?;
    let about = file.with_extension("json");
    let text = std::fs::read_to_string(&about).with_context(|| {
        format!(
            "{} has no {} beside it. Write one saying the size the model takes and how its \
             input is scaled: {{\"width\": 512, \"height\": 288, \"mean\": 0.5, \"std\": 0.5}}",
            file.display(),
            about.display()
        )
    })?;
    let v: serde_json::Value = serde_json::from_str(&text).with_context(|| format!("reading {}", about.display()))?;
    let size = |k: &str| v.get(k).and_then(|n| n.as_u64()).filter(|n| (16..=2048).contains(n)).map(|n| n as usize);
    let (Some(width), Some(height)) = (size("width"), size("height")) else {
        bail!("{} needs a width and a height from 16 to 2048", about.display());
    };
    let num = |k: &str, d: f32| v.get(k).and_then(|n| n.as_f64()).map(|n| n as f32).unwrap_or(d);
    let std = num("std", 1.0);
    if std == 0.0 {
        bail!("{} has a std of 0, which would divide by zero", about.display());
    }
    Ok(Spec { name: name.into(), file, width, height, mean: num("mean", 0.0), std })
}

fn missing(file: &str) -> String {
    format!(
        "no {file} in any models folder ({}). The installer puts the two that ship in the \
         install folder; in a checkout run dev/fetch-models.sh. A model of your own goes in \
         the first of these folders.",
        places().iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
    )
}

/// The first models folder holding `file`.
pub fn find(file: &str) -> Option<PathBuf> {
    places().into_iter().map(|d| d.join(file)).find(|f| f.is_file())
}

/// Where models are looked for, in order: the operator's own folder first,
/// so a model they add wins over one that shipped.
pub fn places() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(dir) = std::env::var_os("GODWINMIX_MODELS_DIR").filter(|d| !d.is_empty()) {
        out.push(PathBuf::from(dir));
    }
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        out.push(Path::new(&home).join(".godwinmix").join("models"));
        out.push(Path::new(&home).join(".cache").join("gmx-models"));
    }
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        out.extend(shipped(&exe_dir));
    }
    #[cfg(target_os = "macos")]
    out.push(PathBuf::from("/Applications/GodwinMix.app/Contents/Resources/models"));
    out
}

/// Where an installer or a checkout puts them, from the mixer's own folder.
pub fn shipped(exe_dir: &Path) -> Vec<PathBuf> {
    let prefix = exe_dir.parent().unwrap_or(exe_dir);
    let mut out = vec![
        exe_dir.join("models"),
        // A Linux package: /usr/bin/godwinmix, /usr/share/godwinmix/models.
        prefix.join("share").join("godwinmix").join("models"),
        // A macOS app: Contents/MacOS/godwinmix, Contents/Resources/models.
        prefix.join("Resources").join("models"),
        // The desktop app's own resources on Linux: /usr/lib/GodwinMix/models.
        prefix.join("lib").join("GodwinMix").join("models"),
    ];
    out.extend(crate::plugin::first_party::checkout_of(exe_dir).map(|c| c.join("models")));
    out
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
