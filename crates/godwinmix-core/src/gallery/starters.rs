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

/// A starter whose files live elsewhere under `graphics/`, named on the
/// left as the item has them: the HTML starter designs are the template
/// pack's own pages, kept once.
macro_rules! starter_from {
    ($id:literal, [$($name:literal <- $from:literal),* $(,)?]) => {
        Starter {
            id: $id,
            files: &[$(($name, include_bytes!(concat!("../../../../graphics/", $from)))),*],
        }
    };
}

/// Every starter that ships.
pub const STARTERS: &[Starter] = &[
    starter!("blue-gradient", ["graphic.toml", "background.svg"]),
    starter_from!("lower-third-glass", ["graphic.toml" <- "starters/lower-third-glass/graphic.toml", "preview.jpg" <- "starters/lower-third-glass/preview.jpg", "lower-third-glass.html" <- "html/lower-third-glass.html"]),
    starter_from!("lower-third-bold", ["graphic.toml" <- "starters/lower-third-bold/graphic.toml", "preview.jpg" <- "starters/lower-third-bold/preview.jpg", "lower-third-bold.html" <- "html/lower-third-bold.html"]),
    starter_from!("lower-third-line", ["graphic.toml" <- "starters/lower-third-line/graphic.toml", "preview.jpg" <- "starters/lower-third-line/preview.jpg", "lower-third-line.html" <- "html/lower-third-line.html"]),
    starter_from!("ticker-crawl", ["graphic.toml" <- "starters/ticker-crawl/graphic.toml", "preview.jpg" <- "starters/ticker-crawl/preview.jpg", "ticker-crawl.html" <- "html/ticker-crawl.html"]),
    starter_from!("ticker-flip", ["graphic.toml" <- "starters/ticker-flip/graphic.toml", "preview.jpg" <- "starters/ticker-flip/preview.jpg", "ticker-flip.html" <- "html/ticker-flip.html"]),
    starter_from!("score-bug-live", ["graphic.toml" <- "starters/score-bug-live/graphic.toml", "preview.jpg" <- "starters/score-bug-live/preview.jpg", "score-bug-live.html" <- "html/score-bug-live.html"]),
    starter_from!("logo-bug-shine", ["graphic.toml" <- "starters/logo-bug-shine/graphic.toml", "preview.jpg" <- "starters/logo-bug-shine/preview.jpg", "logo-bug-shine.html" <- "html/logo-bug-shine.html"]),
    starter_from!("countdown-ring", ["graphic.toml" <- "starters/countdown-ring/graphic.toml", "preview.jpg" <- "starters/countdown-ring/preview.jpg", "countdown-ring.html" <- "html/countdown-ring.html"]),
    starter_from!("logo-spin-3d", ["graphic.toml" <- "starters/logo-spin-3d/graphic.toml", "preview.jpg" <- "starters/logo-spin-3d/preview.jpg", "logo-spin-3d.html" <- "html/logo-spin-3d.html"]),
    starter_from!("title-card-3d", ["graphic.toml" <- "starters/title-card-3d/graphic.toml", "preview.jpg" <- "starters/title-card-3d/preview.jpg", "title-card-3d.html" <- "html/title-card-3d.html"]),
    starter_from!("starting-soon", ["graphic.toml" <- "starters/starting-soon/graphic.toml", "preview.jpg" <- "starters/starting-soon/preview.jpg", "starting-soon.html" <- "html/starting-soon.html"]),
    starter_from!("background-gradient", ["graphic.toml" <- "starters/background-gradient/graphic.toml", "preview.jpg" <- "starters/background-gradient/preview.jpg", "background-gradient.html" <- "html/background-gradient.html"]),
    starter_from!("background-particles", ["graphic.toml" <- "starters/background-particles/graphic.toml", "preview.jpg" <- "starters/background-particles/preview.jpg", "background-particles.html" <- "html/background-particles.html"]),
    starter_from!("studio-newsroom", ["graphic.toml" <- "starters/studio-newsroom/graphic.toml", "preview.jpg" <- "starters/studio-newsroom/preview.jpg", "set-newsroom.html" <- "html/set-newsroom.html", "set-newsroom-desk.svg" <- "set-newsroom-desk.svg"]),
    starter_from!("studio-ring", ["graphic.toml" <- "starters/studio-ring/graphic.toml", "preview.jpg" <- "starters/studio-ring/preview.jpg", "set-studio.html" <- "html/set-studio.html", "set-studio-frame.svg" <- "set-studio-frame.svg"]),
];

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
