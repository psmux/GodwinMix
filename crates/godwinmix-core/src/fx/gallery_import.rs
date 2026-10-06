//! What the gallery's own Import does with a file that is a transition or
//! an effect, so a light leak dropped on the gallery becomes one rather than
//! a clip that darkens the picture or a file it refuses.
//!
//! * A `.glsl`, `.frag` or `.fs` is a shader transition, checked here.
//! * A clip is measured, as `fx.import` measures it. One that is light on
//!   black with no alpha (a leak, a burn, bokeh) becomes an fx item. A clip
//!   with alpha stays the gallery's: it is as likely to be a moving lower
//!   third as a stinger, and the transition picker's Import takes it as a stinger.
//!
//! Anything else is not ours and the gallery decides.

use super::{detect, sprite, toml_form};
use crate::gallery::draft::{Draft, Refusal};
use crate::gallery::Manifest;
use godwinmix_protocol::fx::{FxBlend, FxKind, FxManifest};
use std::path::Path;

const SHADERS: &[&str] = &["glsl", "frag", "fs"];
const CLIPS: &[&str] = &["webm", "mov", "mp4", "m4v", "mkv", "avi"];

/// The draft for `name`, when it is a transition or an effect.
pub fn draft(name: &str, bytes: &[u8]) -> Option<Result<Draft, Refusal>> {
    let ext = Path::new(name).extension()?.to_string_lossy().to_ascii_lowercase();
    if SHADERS.contains(&ext.as_str()) {
        return Some(shader(name, bytes));
    }
    if !CLIPS.contains(&ext.as_str()) {
        return None;
    }
    let dir = std::env::temp_dir().join(format!("gmx-fx-gallery-{}-{}", std::process::id(), super::library::slug(name)));
    let made = clip(name, &ext, bytes, &dir);
    let _ = std::fs::remove_dir_all(&dir);
    made
}

fn shader(name: &str, bytes: &[u8]) -> Result<Draft, Refusal> {
    let source = String::from_utf8_lossy(bytes);
    super::shader::constants(&source).map_err(|e| Refusal::new(format!("{name}: {e}"), "Fix the shader and import it again; docs/reference/fx.md has the form."))?;
    let stem = crate::gallery::draft::stem(name);
    let m = manifest(&stem, FxKind::Shader, FxBlend::Normal, "transition.glsl", 1000);
    finish(&m, vec![("transition.glsl".into(), bytes.to_vec())], None)
}

/// A clip on black becomes an fx item; any other clip is the gallery's.
fn clip(name: &str, ext: &str, bytes: &[u8], dir: &Path) -> Option<Result<Draft, Refusal>> {
    std::fs::create_dir_all(dir).ok()?;
    let file = format!("effect.{ext}");
    std::fs::write(dir.join(&file), bytes).ok()?;
    let measured = detect::measure(&dir.join(&file)).ok()?;
    let v = detect::classify(&measured);
    if measured.alpha || v.kind != FxKind::Overlay {
        return None;
    }
    let mut m = manifest(&crate::gallery::draft::stem(name), v.kind, v.blend, &file, measured.duration_ms.max(1));
    (m.cut_at_measured_ms, m.coverage, m.transition, m.effect) = (v.cut_at_ms, v.coverage, v.transition, true);
    let mut files = vec![(file, bytes.to_vec())];
    if let Ok((strip, still)) = sprite::render(&m, dir, Some(&measured)) {
        files.push(("preview-strip.jpg".into(), strip));
        files.push(("preview.jpg".into(), still));
    }
    Some(finish(&m, files, None))
}

fn manifest(title: &str, kind: FxKind, blend: FxBlend, file: &str, duration_ms: u64) -> FxManifest {
    FxManifest {
        name: super::library::slug(title),
        title: title.trim().to_string(),
        kind,
        blend,
        file: file.to_string(),
        duration_ms,
        cut_at_ms: None,
        cut_at_measured_ms: None,
        coverage: None,
        softness: None,
        invert: false,
        transition: true,
        effect: false,
        licence: None,
        source: None,
    }
}

/// The gallery's draft for an fx manifest and its files.
fn finish(m: &FxManifest, files: Vec<(String, Vec<u8>)>, old: Option<&str>) -> Result<Draft, Refusal> {
    let text = toml_form::render(m, old, "uploaded").map_err(|e| Refusal::new(format!("{e:#}"), "Report this; the item could not be written."))?;
    let manifest = Manifest::parse(&text, "graphic.toml").map_err(|e| Refusal::new(format!("{e:#}"), "Report this; the item could not be read back."))?;
    Ok(Draft { manifest, files, warnings: Vec::new() })
}
