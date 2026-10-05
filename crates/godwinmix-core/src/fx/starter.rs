//! The starter set, compiled in from `graphics/starters/` in the repository
//! and written into the library the first time the library is read.
//!
//! About 85 KB in all: four clips at 640x360 in VP9 (a light leak, bokeh, a
//! glitch with alpha and a film burn), one PNG matte and two shaders, every
//! one made for this project (`dev/make-starter-fx.py`). Written out rather
//! than read from inside the binary because GStreamer opens a file, and only
//! when its folder is missing, so an operator who changes a starter item's
//! cut point keeps it and one who deletes the folder gets it back.

use anyhow::{Context, Result};
use std::path::Path;

macro_rules! item {
    ($name:literal, $file:literal) => {
        (
            $name,
            $file,
            include_bytes!(concat!("../../../../graphics/starters/", $name, "/graphic.toml")) as &[u8],
            include_bytes!(concat!("../../../../graphics/starters/", $name, "/", $file)) as &[u8],
        )
    };
}

/// Name, media file, `graphic.toml`, media.
pub const STARTER: &[(&str, &str, &[u8], &[u8])] = &[
    item!("light-leak", "light-leak.webm"),
    item!("bokeh", "bokeh.webm"),
    item!("glitch", "glitch.webm"),
    item!("film-burn", "film-burn.webm"),
    item!("iris", "iris.png"),
    item!("ripple", "ripple.glsl"),
    item!("glitch-slice", "glitch-slice.glsl"),
];

/// Whether `name` is one of the starter set.
pub fn is_starter(name: &str) -> bool {
    STARTER.iter().any(|(n, ..)| *n == name)
}

/// Write every starter item whose folder is not in `root` yet, and the
/// marker that tells the media library the folder is the gallery's.
pub fn install(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root).with_context(|| format!("making {}", root.display()))?;
    let marker = root.join(".gmx-gallery");
    if !marker.exists() {
        let _ = std::fs::write(&marker, "");
    }
    for (name, file, manifest, media) in STARTER {
        let dir = root.join(name);
        if dir.join(super::library::MANIFEST).is_file() {
            continue;
        }
        std::fs::create_dir_all(&dir).with_context(|| format!("making {}", dir.display()))?;
        std::fs::write(dir.join(file), media).with_context(|| format!("writing {name}/{file}"))?;
        std::fs::write(dir.join(super::library::MANIFEST), manifest).with_context(|| format!("writing {name}/graphic.toml"))?;
    }
    Ok(())
}
