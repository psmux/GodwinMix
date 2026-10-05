//! The starter set, compiled in from `graphics/starters/` in the repository
//! and written into the library the first time the library, or the gallery,
//! is read.
//!
//! About 130 KB in all: four clips at 640x360 in VP9 (a light leak, bokeh, a
//! glitch with alpha and a film burn), one PNG matte and two shaders, every
//! one made for this project (`dev/make-starter-fx.py`), each with its
//! preview still and strip. Written out rather than read from inside the
//! binary because GStreamer opens a file, and only when its folder is
//! missing, so an operator who changes a starter item's cut point keeps it
//! and one who deletes the folder gets it back.

use anyhow::{Context, Result};
use std::path::Path;

/// One starter item: its name, its media file, and every file of its folder
/// with `graphic.toml` last, so a folder is never an item before its media
/// is written.
pub struct Starter {
    pub name: &'static str,
    pub file: &'static str,
    pub files: &'static [(&'static str, &'static [u8])],
}

impl Starter {
    /// The bytes of one of its files.
    pub fn bytes(&self, name: &str) -> Option<&'static [u8]> {
        self.files.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
    }
}

macro_rules! item {
    ($name:literal, $file:literal) => {
        Starter {
            name: $name,
            file: $file,
            files: &[
                ($file, include_bytes!(concat!("../../../../graphics/starters/", $name, "/", $file))),
                ("preview.jpg", include_bytes!(concat!("../../../../graphics/starters/", $name, "/preview.jpg"))),
                ("preview-strip.jpg", include_bytes!(concat!("../../../../graphics/starters/", $name, "/preview-strip.jpg"))),
                ("graphic.toml", include_bytes!(concat!("../../../../graphics/starters/", $name, "/graphic.toml"))),
            ],
        }
    };
}

pub const STARTER: &[Starter] = &[
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
    STARTER.iter().any(|s| s.name == name)
}

/// Write every starter item whose folder is not in `root` yet, and the
/// marker that tells the media library the folder is the gallery's.
pub fn install(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root).with_context(|| format!("making {}", root.display()))?;
    let marker = root.join(crate::gallery::MARKER);
    if !marker.exists() {
        let _ = std::fs::write(&marker, "");
    }
    for s in STARTER {
        let dir = root.join(s.name);
        if dir.join(super::library::MANIFEST).is_file() {
            continue;
        }
        std::fs::create_dir_all(&dir).with_context(|| format!("making {}", dir.display()))?;
        for (file, bytes) in s.files {
            std::fs::write(dir.join(file), bytes).with_context(|| format!("writing {}/{file}", s.name))?;
        }
    }
    Ok(())
}
