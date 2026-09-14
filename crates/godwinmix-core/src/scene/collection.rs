//! A collection as a thing you can hand to somebody else.
//!
//! The document on its own is not a share: it names assets by relative path
//! and graphics and filters by plugin, and the person opening it has neither.
//! So an export is a directory, and a zip of that directory:
//!
//! ```text
//!   collection.json      the document, exactly as the store writes it
//!   bundle.json          what an importer needs: what it requires, what the
//!                        assets hash to, and what could not be carried
//!   assets/...           the files, at the relative paths the document names
//! ```
//!
//! Two rules, both from 11 section 7, and both paid for by somebody else's
//! failure. Every asset path is relative: OBS stores absolute ones, which is
//! why every commercial scene bundle ships a relink wizard and why a show
//! moved between two machines opens with black rectangles. And a partial
//! import succeeds visibly: an asset that is missing or that does not match
//! its hash lands in a relink report naming the file and the items that use
//! it, rather than being guessed at or quietly dropped.
//!
//! The zip is `crate::zip`, store only, which is why an export of a folder of
//! PNGs is written at memcpy speed and opens in Finder, Explorer and `unzip`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::scene::document::{Asset, Collection, Content, Item, Scene};
use crate::scene::id::Id;
use crate::zip::Zip;

/// The bundle format this build writes. Separate from the document's own
/// `schemaVersion`: the document can change shape without the envelope
/// changing, and an importer needs to know which it is looking at.
pub const BUNDLE_VERSION: u32 = 1;

/// The name of the document inside a bundle.
pub const DOCUMENT: &str = "collection.json";
/// The name of the envelope inside a bundle.
pub const MANIFEST: &str = "bundle.json";
/// Where the files live inside a bundle.
pub const ASSETS: &str = "assets";

/// What an importer is told before it reads the document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Bundle {
    /// The envelope version. See [`BUNDLE_VERSION`].
    pub bundle_version: u32,
    /// The collection's own stable id, repeated here so a listing can be read
    /// without unpacking the document.
    pub id: Id,
    pub name: String,
    pub canvas: crate::scene::document::Canvas,
    /// The build that wrote it, for a bug report.
    pub written_by: String,
    /// Every plugin this collection needs, with the version range that will
    /// do. An importer that has none of them still gets the geometry.
    pub requires: Vec<Requirement>,
    /// Every file carried, by the path inside the bundle.
    pub assets: Vec<BundleAsset>,
    /// What could not be carried, one line each, so a partial export is
    /// visible rather than silent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<String>,
}

/// One plugin the collection needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Requirement {
    /// The plugin name, `ograf`.
    pub plugin: String,
    /// A semver range, `^0.2.0`, or `*` when the exporter had no version to
    /// name because the plugin was not installed where the export ran.
    pub versions: String,
    /// The provide ids used, `ograf/lower-third`, so a reader can see what the
    /// collection actually asks the plugin for.
    pub provides: Vec<String>,
}

/// One file carried in the bundle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BundleAsset {
    /// The asset id in the document.
    pub id: Id,
    /// Relative to the bundle root, forward slashes. Never absolute: see the
    /// head of this module.
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

/// One asset an import could not put back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Relink {
    pub asset: Id,
    /// The path the document asks for.
    pub path: String,
    /// Why it could not be used: missing, or a hash that does not match.
    pub reason: String,
    /// The items that draw it, by scene and item name, so the person fixing
    /// it knows what will be blank until they do.
    pub items: Vec<String>,
}

/// What one import produced.
#[derive(Debug, Clone)]
pub struct Imported {
    pub document: Collection,
    pub bundle: Bundle,
    /// Empty when every asset came across.
    pub relink: Vec<Relink>,
    /// Where the assets were written, when they were written anywhere.
    pub assets_at: Option<PathBuf>,
}

/// Everything an export needs that is not in the document.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Where the document's relative asset paths resolve from: the directory
    /// the collection lives in.
    pub root: Option<PathBuf>,
    /// Installed plugin versions, `ograf` to `0.2.0`, so the bundle can say
    /// what it was built against. A plugin missing here is recorded as `*`.
    pub versions: BTreeMap<String, String>,
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// Write a collection as a directory.
///
/// Returns the manifest that was written, so a caller can report what went in
/// without reading the directory back.
pub fn export_dir(doc: &Collection, to: &Path, options: &Options) -> Result<Bundle> {
    let (bundle, files) = build(doc, options)?;
    std::fs::create_dir_all(to)
        .with_context(|| format!("making {} for the export", to.display()))?;
    std::fs::write(to.join(DOCUMENT), doc.to_json())
        .with_context(|| format!("writing {}", to.join(DOCUMENT).display()))?;
    let manifest = serde_json::to_string_pretty(&bundle)? + "\n";
    std::fs::write(to.join(MANIFEST), manifest)
        .with_context(|| format!("writing {}", to.join(MANIFEST).display()))?;
    for (path, bytes) in &files {
        let at = to.join(path);
        if let Some(dir) = at.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("making {} for an asset", dir.display()))?;
        }
        std::fs::write(&at, bytes).with_context(|| format!("writing {}", at.display()))?;
    }
    Ok(bundle)
}

/// Write a collection as a zip, in memory.
pub fn export_zip(doc: &Collection, options: &Options) -> Result<(Bundle, Vec<u8>)> {
    let (bundle, files) = build(doc, options)?;
    let mut zip = Zip::new();
    zip.add(DOCUMENT, doc.to_json().as_bytes());
    zip.add(MANIFEST, (serde_json::to_string_pretty(&bundle)? + "\n").as_bytes());
    for (path, bytes) in &files {
        // A name already taken is refused by the writer. Two assets at one
        // path cannot happen (the map is keyed by path) so this cannot lose a
        // file, but it is worth not asserting on somebody else's document.
        zip.add(path, bytes);
    }
    Ok((bundle, zip.finish()))
}

/// The manifest and the file bytes, which is everything both exports share.
fn build(doc: &Collection, options: &Options) -> Result<(Bundle, BTreeMap<String, Vec<u8>>)> {
    let mut assets = Vec::new();
    let mut files = BTreeMap::new();
    let mut skipped = Vec::new();
    for (id, asset) in &doc.assets {
        match read_asset(asset, options.root.as_deref()) {
            Ok(bytes) => {
                let path = format!("{ASSETS}/{}", asset.path.trim_start_matches('/'));
                assets.push(BundleAsset {
                    id: *id,
                    sha256: godwinmix_host::verify::sha256::hex(&bytes),
                    size: bytes.len() as u64,
                    path: path.clone(),
                });
                files.insert(path, bytes);
            }
            Err(why) => skipped.push(format!("{}: {why}", asset.path)),
        }
    }
    let bundle = Bundle {
        bundle_version: BUNDLE_VERSION,
        id: doc.id,
        name: doc.name.clone(),
        canvas: doc.canvas,
        written_by: format!("godwinmix {}", env!("CARGO_PKG_VERSION")),
        requires: requirements(doc, &options.versions),
        assets,
        skipped,
    };
    Ok((bundle, files))
}

/// Read one asset, refusing an absolute path and a path that climbs out.
///
/// Refused here rather than at import: an export that carried
/// `/home/alice/logo.png` would open on the other machine with a relink
/// report and no file, and the person who can fix it is the one exporting.
fn read_asset(asset: &Asset, root: Option<&Path>) -> Result<Vec<u8>> {
    let path = Path::new(&asset.path);
    if path.is_absolute() || asset.path.contains("..") {
        bail!(
            "the document gives this asset an absolute or climbing path. A collection \
             carries its files by a path relative to its own directory, so move the file \
             beside the collection and point the asset at it"
        );
    }
    let root = root.context(
        "this collection has assets but no directory to resolve them against. Save it \
         first, or export from a core with a runtime store",
    )?;
    let at = root.join(path);
    std::fs::read(&at).with_context(|| format!("reading {}", at.display()))
}

/// Every plugin the document names, with the version range to ask for.
fn requirements(doc: &Collection, versions: &BTreeMap<String, String>) -> Vec<Requirement> {
    let mut used: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut note = |type_id: &str| {
        let plugin = type_id.split('/').next().unwrap_or(type_id).to_string();
        if plugin.is_empty() {
            return;
        }
        used.entry(plugin).or_default().insert(type_id.to_string());
    };
    for scene in &doc.scenes {
        for item in scene.walk() {
            if let Content::Graphic { graphic, .. } = &item.content {
                note(graphic);
            }
            for filter in &item.filters {
                note(&filter.kind);
            }
        }
    }
    for transition in &doc.transitions {
        note(&transition.kind);
    }
    used.into_iter()
        .map(|(plugin, provides)| Requirement {
            versions: match versions.get(&plugin) {
                // A caret range: the collection was built against this version
                // and anything compatible with it will do.
                Some(v) => format!("^{v}"),
                None => "*".into(),
            },
            plugin,
            provides: provides.into_iter().collect(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

/// Read a bundle: a zip, or a directory that was one.
///
/// `assets_to` is where the files are written. With none, the assets are
/// checked and not written, which is what a dry run wants.
pub fn import(path: &Path, assets_to: Option<&Path>) -> Result<Imported> {
    let members = if path.is_dir() { read_dir(path)? } else { read_zip(path)? };
    let text = members.get(DOCUMENT).with_context(|| {
        format!(
            "{} holds no {DOCUMENT}, so it is not a collection bundle. \
             Pass the .zip that scene.export wrote, or the directory it unpacks to.",
            path.display()
        )
    })?;
    let document = Collection::from_json(&String::from_utf8_lossy(text))
        .with_context(|| format!("reading the collection in {}", path.display()))?;
    let bundle = match members.get(MANIFEST) {
        Some(bytes) => serde_json::from_slice::<Bundle>(bytes)
            .with_context(|| format!("reading the {MANIFEST} in {}", path.display()))?,
        // A bare collection.json is a valid thing to hand somebody. It carries
        // no envelope, so one is made from the document itself.
        None => Bundle {
            bundle_version: BUNDLE_VERSION,
            id: document.id,
            name: document.name.clone(),
            canvas: document.canvas,
            written_by: "unknown".into(),
            requires: requirements(&document, &BTreeMap::new()),
            assets: Vec::new(),
            skipped: Vec::new(),
        },
    };
    if bundle.bundle_version > BUNDLE_VERSION {
        bail!(
            "this bundle is version {} and this build reads {BUNDLE_VERSION}. \
             Upgrade GodwinMix, or export it again from the core that wrote it \
             asking for an older format.",
            bundle.bundle_version
        );
    }
    let (relink, assets_at) = place_assets(&document, &members, assets_to)?;
    Ok(Imported { document, bundle, relink, assets_at })
}

/// Check every asset the document names and write the ones that came across.
fn place_assets(
    doc: &Collection,
    members: &BTreeMap<String, Vec<u8>>,
    to: Option<&Path>,
) -> Result<(Vec<Relink>, Option<PathBuf>)> {
    let mut relink = Vec::new();
    let mut written = false;
    for (id, asset) in &doc.assets {
        let inside = format!("{ASSETS}/{}", asset.path.trim_start_matches('/'));
        let Some(bytes) = members.get(&inside) else {
            relink.push(missing(doc, *id, asset, "the bundle does not carry this file"));
            continue;
        };
        if let Some(want) = &asset.sha256 {
            let have = godwinmix_host::verify::sha256::hex(bytes);
            if !have.eq_ignore_ascii_case(want) {
                relink.push(missing(
                    doc,
                    *id,
                    asset,
                    "the file in the bundle is not the file the document names",
                ));
                continue;
            }
        }
        let Some(to) = to else { continue };
        let at = to.join(&asset.path);
        if let Some(dir) = at.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("making {} for an asset", dir.display()))?;
        }
        std::fs::write(&at, bytes).with_context(|| format!("writing {}", at.display()))?;
        written = true;
    }
    Ok((relink, to.filter(|_| written).map(Path::to_path_buf)))
}

/// One line of the relink report, with the items that will be blank until it
/// is fixed.
fn missing(doc: &Collection, id: Id, asset: &Asset, reason: &str) -> Relink {
    Relink {
        asset: id,
        path: asset.path.clone(),
        reason: reason.into(),
        items: users(doc, &id.to_string(), &asset.path),
    }
}

/// Every item that mentions this asset, by scene and item name.
///
/// By text search through the item's own parameters, because an asset is
/// referred to by its id or by its path and neither is a typed field: a
/// graphic puts it in `params.image`, a filter in `params.mask`, and a plugin
/// nobody here has heard of puts it wherever its schema says.
fn users(doc: &Collection, id: &str, path: &str) -> Vec<String> {
    let mut out = Vec::new();
    for scene in &doc.scenes {
        for item in scene.walk() {
            if mentions(item, id, path) {
                out.push(format!("{}: {}", scene.name, label(scene, item)));
            }
        }
    }
    out
}

fn mentions(item: &Item, id: &str, path: &str) -> bool {
    let mut text = String::new();
    if let Content::Graphic { graphic, params } = &item.content {
        text.push_str(graphic);
        text.push_str(&params.to_string());
    }
    for filter in &item.filters {
        text.push_str(&filter.params.to_string());
    }
    text.contains(id) || text.contains(path)
}

/// What to call an item in a report: its name, else its place in the scene.
fn label(scene: &Scene, item: &Item) -> String {
    match &item.name {
        Some(name) => name.clone(),
        None => {
            let n = scene.walk().iter().position(|i| i.id == item.id).unwrap_or(0) + 1;
            format!("item {n} ({})", item.id)
        }
    }
}

/// Every file in a directory, keyed by its path relative to the root.
fn read_dir(root: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    fn walk(root: &Path, at: &Path, out: &mut BTreeMap<String, Vec<u8>>) -> Result<()> {
        for entry in std::fs::read_dir(at)
            .with_context(|| format!("reading {}", at.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out)?;
                continue;
            }
            let name = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(name, std::fs::read(&path)?);
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out)?;
    Ok(out)
}

fn read_zip(path: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    let bytes = std::fs::read(path).with_context(|| {
        format!(
            "reading {}. Give the path to the .zip that scene.export wrote, or to the \
             directory it unpacks to.",
            path.display()
        )
    })?;
    crate::zip::read(&bytes).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests;
