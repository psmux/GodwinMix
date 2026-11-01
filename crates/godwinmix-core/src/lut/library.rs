//! The LUT library: what ships, what was imported, and the saved presets.
//!
//! It is the `luts/` folder in the gallery's folder. Each imported LUT is
//! kept there as `<name>.cube`, whatever it came in as, so a frame's thread
//! never decodes a picture and a collection carries its looks with it.
//! Presets are `presets.toml` beside them: a LUT, an amount and a colour
//! correction under one name, which is what `lut/filter` takes as `preset`.

use super::looks::{self, Group};
use super::{cube, Lut};
use anyhow::{bail, Context, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;

pub const PRESETS: &str = "presets.toml";

pub fn dir() -> PathBuf {
    crate::gallery::dir().join("luts")
}

/// One LUT the library has.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entry {
    pub name: String,
    pub title: String,
    /// `camera`, `look` or `imported`.
    pub group: String,
    /// Points a side.
    pub size: usize,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub about: String,
}

/// A saved preset. Anything left out is the filter's default.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lut: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contrast: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gamma: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saturation: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hue_shift: Option<f64>,
}

#[derive(Default, Serialize, Deserialize)]
struct PresetFile {
    #[serde(default)]
    preset: Vec<Preset>,
}

/// Every LUT, the built ins first, and a sentence for each file that would
/// not read.
pub fn list(dir: &Path) -> (Vec<Entry>, Vec<String>) {
    let mut out: Vec<Entry> = looks::ALL
        .iter()
        .map(|b| Entry { name: b.name.into(), title: b.title.into(), group: b.group.as_str().into(), size: looks::SIZE, about: b.about.into() })
        .collect();
    let mut errors = Vec::new();
    let mut imported = Vec::new();
    for path in std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()) {
        let Some(name) = cube_name(&path) else { continue };
        match header(&path) {
            Ok((title, size)) => imported.push(Entry { title: if title.is_empty() { name.clone() } else { title }, name, group: "imported".into(), size, about: String::new() }),
            Err(e) => errors.push(format!("{}: {e:#}", path.display())),
        }
    }
    imported.sort_by(|a, b| a.name.cmp(&b.name));
    out.extend(imported);
    (out, errors)
}

fn cube_name(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    (path.extension()? == "cube" && crate::gallery::slug(stem) == stem).then(|| stem.to_string())
}

/// The title and size from a `.cube`'s header, without reading its rows.
fn header(path: &Path) -> Result<(String, usize)> {
    use std::io::BufRead;
    let file = std::fs::File::open(path)?;
    let (mut title, mut size) = (String::new(), 0);
    for line in std::io::BufReader::new(file).lines().take(64) {
        let line = line?;
        let l = line.trim();
        if let Some(t) = l.strip_prefix("TITLE") {
            title = t.trim().trim_matches('"').to_string();
        } else if let Some(n) = l.strip_prefix("LUT_3D_SIZE").or_else(|| l.strip_prefix("LUT_1D_SIZE")) {
            size = size.max(n.trim().parse().unwrap_or(0));
        } else if l.starts_with(|c: char| c.is_ascii_digit() || c == '-') {
            break;
        }
    }
    if size == 0 {
        bail!("no LUT_3D_SIZE or LUT_1D_SIZE in the first lines");
    }
    Ok((title, size))
}

/// Every LUT read so far, by file and when it was written, so moving an
/// amount slider does not read a 65 point file again.
fn cache() -> &'static Mutex<HashMap<PathBuf, (SystemTime, Arc<Lut>)>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, (SystemTime, Arc<Lut>)>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// The LUT called `name`: a built in, or one imported into `dir`.
pub fn load(dir: &Path, name: &str) -> Result<Arc<Lut>> {
    if let Some(b) = looks::find(name) {
        let key = PathBuf::from(format!("built-in:{}", b.name));
        if let Some((_, lut)) = cache().lock().get(&key) {
            return Ok(lut.clone());
        }
        let lut = Arc::new(b.lut());
        cache().lock().insert(key, (SystemTime::UNIX_EPOCH, lut.clone()));
        return Ok(lut);
    }
    let path = dir.join(format!("{}.cube", crate::gallery::slug(name)));
    if !path.is_file() {
        let (all, _) = list(dir);
        let names: Vec<&str> = all.iter().map(|e| e.name.as_str()).collect();
        bail!("there is no LUT called {name:?}. The library has: {}. Import one with lut.import", names.join(", "));
    }
    load_file(&path)
}

/// A LUT file anywhere, read once and kept while it is unchanged.
pub fn load_file(path: &Path) -> Result<Arc<Lut>> {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).with_context(|| format!("reading {}", path.display()))?;
    if let Some((when, lut)) = cache().lock().get(path) {
        if *when == modified {
            return Ok(lut.clone());
        }
    }
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let lut = Arc::new(super::parse(&name, &bytes).with_context(|| format!("reading {}", path.display()))?);
    cache().lock().insert(path.to_path_buf(), (modified, lut.clone()));
    Ok(lut)
}

/// Read the LUT at `src` and keep it in the library as `<name>.cube`.
pub fn import(dir: &Path, src: &Path, name: Option<&str>, title: Option<&str>) -> Result<Entry> {
    let file_name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let bytes = std::fs::read(src).with_context(|| format!("reading {}", src.display()))?;
    let mut lut = super::parse(&file_name, &bytes).with_context(|| format!("reading {}", src.display()))?;
    let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let slug = crate::gallery::slug(name.unwrap_or(&stem));
    if slug.is_empty() {
        bail!("{file_name} gives no name to keep it under. Pass a name, such as \"church-warm\"");
    }
    if looks::find(&slug).is_some() {
        bail!("{slug} is the name of a built in LUT. Pass another name, such as \"{slug}-custom\"");
    }
    if let Some(t) = title.filter(|t| !t.trim().is_empty()) {
        lut.title = t.trim().to_string();
    } else if lut.title.is_empty() {
        lut.title = stem;
    }
    std::fs::create_dir_all(dir).with_context(|| format!("making {}", dir.display()))?;
    write_atomic(&dir.join(format!("{slug}.cube")), &cube::write(&lut))?;
    Ok(Entry { name: slug, title: lut.title.clone(), group: "imported".into(), size: lut.size(), about: String::new() })
}

pub fn remove(dir: &Path, name: &str) -> Result<()> {
    if looks::find(name).is_some() {
        bail!("{name} is built in and cannot be removed. Leave it unused, or save a preset that names another LUT");
    }
    let path = dir.join(format!("{}.cube", crate::gallery::slug(name)));
    if !path.is_file() {
        bail!("there is no imported LUT called {name:?}. lut.list shows what there is");
    }
    std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
    cache().lock().remove(&path);
    Ok(())
}

pub fn presets(dir: &Path) -> Result<Vec<Preset>> {
    let path = dir.join(PRESETS);
    let Ok(text) = std::fs::read_to_string(&path) else { return Ok(Vec::new()) };
    let file: PresetFile = toml::from_str(&text).with_context(|| format!("reading {}", path.display()))?;
    Ok(file.preset)
}

pub fn preset(dir: &Path, name: &str) -> Result<Preset> {
    let all = presets(dir)?;
    let wanted = crate::gallery::slug(name);
    if let Some(p) = all.iter().find(|p| p.name == wanted) {
        return Ok(p.clone());
    }
    let names: Vec<&str> = all.iter().map(|p| p.name.as_str()).collect();
    bail!(
        "there is no LUT preset called {name:?}. {}Save one with lut.preset.save",
        if names.is_empty() { "None are saved yet. ".to_string() } else { format!("The saved ones are: {}. ", names.join(", ")) }
    )
}

/// Save `p`, replacing a preset of the same name. The LUT it names must be
/// in the library.
pub fn save_preset(dir: &Path, mut p: Preset) -> Result<Preset> {
    p.name = crate::gallery::slug(&p.name);
    if p.name.is_empty() {
        bail!("a preset needs a name, such as \"church-warm\"");
    }
    if let Some(lut) = &p.lut {
        load(dir, lut)?;
    }
    if let Some(a) = p.amount {
        if !(0.0..=1.0).contains(&a) {
            bail!("amount is {a}, and it goes from 0 (none of the LUT) to 1 (all of it)");
        }
    }
    let mut all = presets(dir)?;
    all.retain(|q| q.name != p.name);
    all.push(p.clone());
    all.sort_by(|a, b| a.name.cmp(&b.name));
    write_presets(dir, all)?;
    Ok(p)
}

pub fn remove_preset(dir: &Path, name: &str) -> Result<()> {
    let mut all = presets(dir)?;
    let before = all.len();
    let wanted = crate::gallery::slug(name);
    all.retain(|p| p.name != wanted);
    if all.len() == before {
        bail!("there is no LUT preset called {name:?}. lut.list shows the saved ones");
    }
    write_presets(dir, all)
}

fn write_presets(dir: &Path, preset: Vec<Preset>) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("making {}", dir.display()))?;
    let text = toml::to_string_pretty(&PresetFile { preset }).context("writing the presets")?;
    write_atomic(&dir.join(PRESETS), &text)
}

fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))
}

/// Built in names grouped, for a sentence that lists them.
pub fn built_in_names(group: Group) -> Vec<&'static str> {
    looks::ALL.iter().filter(|b| b.group == group).map(|b| b.name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gmx-lut-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn an_import_lists_and_loads_and_removes() {
        let d = scratch("import");
        let src = d.join("My Grade.cube");
        std::fs::write(&src, cube::write(&Lut::from_fn("", 3, |c| c))).unwrap();
        let lib = d.join("luts");
        let e = import(&lib, &src, None, None).unwrap();
        assert_eq!((e.name.as_str(), e.title.as_str(), e.size), ("my-grade", "My Grade", 3));
        let (all, errors) = list(&lib);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(all.iter().any(|x| x.name == "my-grade" && x.group == "imported"));
        assert!(all.iter().any(|x| x.name == "slog3-to-709" && x.group == "camera"));
        assert_eq!(load(&lib, "my-grade").unwrap().size(), 3);
        remove(&lib, "my-grade").unwrap();
        assert!(load(&lib, "my-grade").unwrap_err().to_string().contains("lut.import"));
    }

    #[test]
    fn a_built_in_name_cannot_be_taken_or_removed() {
        let d = scratch("builtin");
        let src = d.join("warm.cube");
        std::fs::write(&src, cube::write(&Lut::from_fn("", 2, |c| c))).unwrap();
        assert!(import(&d, &src, None, None).unwrap_err().to_string().contains("warm-custom"));
        assert!(remove(&d, "warm").is_err());
    }

    #[test]
    fn presets_save_replace_and_go() {
        let d = scratch("presets");
        let p = Preset { name: "Church Warm".into(), lut: Some("warm".into()), amount: Some(0.6), ..Default::default() };
        assert_eq!(save_preset(&d, p.clone()).unwrap().name, "church-warm");
        save_preset(&d, Preset { amount: Some(0.8), ..p }).unwrap();
        let all = presets(&d).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].amount, Some(0.8));
        remove_preset(&d, "church-warm").unwrap();
        assert!(preset(&d, "church-warm").unwrap_err().to_string().contains("lut.preset.save"));
    }

    #[test]
    fn a_preset_naming_a_missing_lut_is_refused() {
        let d = scratch("missing");
        let p = Preset { name: "x".into(), lut: Some("nope".into()), ..Default::default() };
        assert!(save_preset(&d, p).is_err());
    }
}
