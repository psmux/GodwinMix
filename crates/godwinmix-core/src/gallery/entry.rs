//! One listed item and where it really is: a folder in the gallery, a
//! starter compiled in, a pack template, or an SVG template in the media
//! library. The wire's `GalleryItem` is worked out here from the manifest
//! and the file, filling in what the manifest left out.

use super::detect;
use super::manifest::Manifest;
use super::starters::Starter;
use godwinmix_protocol::gallery::{GalleryItem, GalleryKind, Origin};
use std::path::{Path, PathBuf};

/// Where an item's files are.
#[derive(Clone)]
pub enum Where {
    /// A folder: saved, imported, or a starter written out.
    Folder(PathBuf),
    /// A template from the pack, by name.
    Pack(&'static str),
    /// An SVG template in the media library, by file name.
    Library(String),
}

#[derive(Clone)]
pub struct Entry {
    pub item: GalleryItem,
    pub manifest: Manifest,
    pub at: Where,
    /// The starter this is, when it is one.
    pub starter: Option<&'static Starter>,
}

impl std::fmt::Debug for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Entry").field("id", &self.item.id).field("kind", &self.item.kind).finish()
    }
}

impl Entry {
    /// An item in a folder.
    pub fn folder(id: &str, dir: PathBuf, manifest: Manifest, starter: Option<&'static Starter>) -> Entry {
        let file = (!manifest.file.is_empty()).then(|| dir.join(&manifest.file));
        let item = item(id, &manifest, file.as_deref());
        let mut e = Entry { item, manifest, at: Where::Folder(dir), starter };
        let own_clip = matches!(e.item.kind, GalleryKind::Clip | GalleryKind::Transition | GalleryKind::Effect)
            && [".webm", ".mp4", ".mov"].iter().any(|x| e.manifest.file.to_ascii_lowercase().ends_with(x));
        e.item.moving = e.preview_loop().or_else(|| own_clip.then(|| e.manifest.file.clone()));
        e
    }

    /// The main file on disk, when the item has one there.
    pub fn file(&self) -> Option<PathBuf> {
        match &self.at {
            Where::Folder(dir) if !self.manifest.file.is_empty() => Some(dir.join(&self.manifest.file)),
            Where::Library(name) => crate::graphics::brand::library().map(|d| d.join(name)),
            _ => None,
        }
    }

    pub fn dir(&self) -> Option<&Path> {
        match &self.at {
            Where::Folder(dir) => Some(dir),
            _ => None,
        }
    }

    /// Shipped items are changed by duplicating them.
    pub fn read_only(&self) -> bool {
        self.item.origin == Origin::Shipped
    }

    /// A number that changes whenever the item or its main file does, for
    /// the preview cache.
    pub fn stamp(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.manifest.to_toml().unwrap_or_default().hash(&mut h);
        if let Where::Pack(name) = &self.at {
            name.hash(&mut h);
        }
        for path in self.file().into_iter().chain(self.preview_file()) {
            if let Ok(meta) = std::fs::metadata(&path) {
                meta.len().hash(&mut h);
                meta.modified().ok().hash(&mut h);
            }
        }
        h.finish()
    }

    /// The item's own still, `preview.jpg`, `.png` or `.webp`, when it has one.
    pub fn preview_file(&self) -> Option<PathBuf> {
        let dir = self.dir()?;
        ["preview.jpg", "preview.png", "preview.webp"].iter().map(|n| dir.join(n)).find(|p| p.is_file())
    }

    /// The item's own moving preview, when it has one.
    pub fn preview_loop(&self) -> Option<String> {
        let dir = self.dir()?;
        ["preview.webm", "preview.mp4"].iter().find(|n| dir.join(n).is_file()).map(|n| n.to_string())
    }
}

/// The wire's description, with every default filled in.
pub fn item(id: &str, m: &Manifest, file: Option<&Path>) -> GalleryItem {
    let kind = m.kind().unwrap_or(GalleryKind::Image);
    let transparent = m.transparent.unwrap_or_else(|| detect::transparent(kind, file));
    let zone = m.zone().unwrap_or_else(|| detect::zone(kind, transparent, file.and_then(detect::size)));
    let fields = match (kind, file) {
        (GalleryKind::Template, Some(f)) => crate::graphics::pack::load(&f.display().to_string()).map(|t| t.info.fields).unwrap_or_default(),
        _ => Vec::new(),
    };
    let uri = match kind {
        GalleryKind::Template => file.map(|f| format!("template:{}", plain(f))),
        GalleryKind::Image | GalleryKind::Clip => file.map(plain),
        GalleryKind::Ticker | GalleryKind::Text => m.source.as_ref().map(|s| s.uri.clone()),
        _ => None,
    };
    GalleryItem {
        id: id.to_string(),
        name: m.name.clone(),
        kind,
        zone,
        moves: m.moves.unwrap_or_else(|| detect::moves(kind)),
        transparent,
        origin: m.origin(),
        description: m.description.clone(),
        tags: m.tags.clone(),
        fields,
        values: m.values_json(),
        uri,
        made_by: m.made_by.clone(),
        saved: m.saved.clone(),
        moving: None,
        placed: Vec::new(),
    }
}

/// A path as a source reads it: absolute, and never Windows' `\\?\` form,
/// which the kinds that read an address by its shape do not recognise.
pub fn plain(path: &Path) -> String {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let text = abs.to_string_lossy().into_owned();
    text.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(text)
}
