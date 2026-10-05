//! The starter designs compiled into the mixer, from `graphics/starters/`.
//!
//! Each is a gallery item folder, the same shape a saved one has, held as
//! bytes so every install has them with nothing to copy. They are listed
//! read only. The first time one is drawn or placed it is written out under
//! `<gallery>/.shipped/<id>/`, because a source reads a file, and written
//! again whenever the binary carries a different copy.
//!
//! Adding a starter is a folder under `graphics/starters/` and a line here
//! for each of its files. There is no build script and no glob, as with the
//! template pack: the table is the list of what ships.

use super::manifest::Manifest;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// One starter: its id and its files, `graphic.toml` among them.
pub struct Starter {
    pub id: &'static str,
    pub files: &'static [(&'static str, &'static [u8])],
}

macro_rules! starter {
    ($id:literal, [$($file:literal),* $(,)?]) => {
        Starter {
            id: $id,
            files: &[$(($file, include_bytes!(concat!("../../../../graphics/starters/", $id, "/", $file)))),*],
        }
    };
}

/// Every starter that ships.
pub const STARTERS: &[Starter] = &[starter!("blue-gradient", ["graphic.toml", "background.svg"])];

impl Starter {
    pub fn manifest(&self) -> Result<Manifest> {
        let (_, toml) = self.files.iter().find(|(n, _)| *n == super::MANIFEST).context("a starter with no graphic.toml")?;
        let mut m = Manifest::parse(&String::from_utf8_lossy(toml), &format!("starter {}", self.id))?;
        m.origin = Some("shipped".into());
        Ok(m)
    }

    /// The folder this starter is written out to.
    pub fn folder(&self, gallery: &Path) -> PathBuf {
        gallery.join(".shipped").join(self.id)
    }

    /// Write the starter out when it is not there or differs, and answer
    /// its folder.
    pub fn materialise(&self, gallery: &Path) -> Result<PathBuf> {
        let dir = self.folder(gallery);
        for (name, bytes) in self.files {
            let path = dir.join(name);
            if std::fs::read(&path).ok().as_deref() == Some(*bytes) {
                continue;
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).with_context(|| format!("making {}", parent.display()))?;
            }
            std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
        }
        Ok(dir)
    }
}

/// The starter with this id.
pub fn find(id: &str) -> Option<&'static Starter> {
    STARTERS.iter().find(|s| s.id == id)
}
