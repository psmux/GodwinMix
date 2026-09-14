//! Where the collection lives on disk.
//!
//! One file, `<stem>.scenes.json` beside the runtime store, written as the
//! nested tree so a diff in git reads like the picture it describes. Written
//! on change, through a temporary file and a rename, because a half written
//! scene collection is a show that will not load.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::scene::document::{Canvas, Collection};

/// The scene collection's path, given the runtime store's.
///
/// `runtime.json` becomes `runtime.scenes.json`. Beside it rather than inside
/// it because the two have different lifetimes: the runtime store is what this
/// core was last doing, the collection is the show, and somebody copying a
/// show between machines copies one and not the other.
pub fn path_beside(runtime_store: &Path) -> PathBuf {
    let stem = runtime_store.file_stem().map(|s| s.to_string_lossy().to_string());
    match stem {
        Some(stem) => runtime_store.with_file_name(format!("{stem}.scenes.json")),
        None => runtime_store.with_extension("scenes.json"),
    }
}

/// Read the collection, or start an empty one at this canvas.
///
/// A file that will not parse is a loud failure and not a silent new document:
/// somebody's show is in there, and overwriting it with an empty one because
/// a key was misspelled is the worst thing this could do.
pub fn load(path: &Path, canvas: Canvas) -> Result<Collection> {
    if !path.exists() {
        return Ok(Collection::new("Scenes", canvas));
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading the scene collection in {}", path.display()))?;
    let mut doc = Collection::from_json(&text)
        .with_context(|| format!("reading the scene collection in {}", path.display()))?;
    // The canvas the core is actually running at wins. A collection authored
    // at 1080p and opened on a core running 720p is the ordinary case, and the
    // layouts resolve against the running canvas.
    doc.canvas = canvas;
    Ok(doc)
}

/// Write the collection, atomically.
pub fn save(path: &Path, doc: &Collection) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("making {} for the scene collection", dir.display()))?;
    }
    let temp = path.with_extension("scenes.json.tmp");
    std::fs::write(&temp, doc.to_json())
        .with_context(|| format!("writing {}", temp.display()))?;
    std::fs::rename(&temp, path)
        .with_context(|| format!("moving the scene collection into {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::document::{Content, Item, Scene};

    #[test]
    fn the_collection_sits_beside_the_runtime_store() {
        assert_eq!(
            path_beside(Path::new("/var/lib/gmx/runtime.json")),
            PathBuf::from("/var/lib/gmx/runtime.scenes.json")
        );
    }

    #[test]
    fn a_missing_file_is_an_empty_collection_and_a_broken_one_is_an_error() {
        let dir = std::env::temp_dir().join(format!("gmx-scenes-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("runtime.scenes.json");
        let _ = std::fs::remove_file(&path);

        let empty = load(&path, Canvas::default()).expect("a missing file starts empty");
        assert!(empty.scenes.is_empty());

        let mut doc = empty;
        let mut scene = Scene::new("wide");
        scene.items.push(Item::new(Content::Source { source: "cam1".into() }));
        doc.scenes.push(scene);
        save(&path, &doc).expect("writing");
        let back = load(&path, Canvas::default()).expect("reading it back");
        assert_eq!(back.scenes.len(), 1);
        assert_eq!(back.scenes[0].name, "wide");

        std::fs::write(&path, "{ this is not json").unwrap();
        let err = load(&path, Canvas::default()).expect_err("a broken file must not be overwritten");
        assert!(format!("{err:#}").contains("scene collection"), "{err:#}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_running_canvas_wins_over_the_one_in_the_file() {
        let dir = std::env::temp_dir().join(format!("gmx-canvas-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("runtime.scenes.json");
        save(&path, &Collection::new("show", Canvas { width: 1920, height: 1080, fps: 30 })).unwrap();
        let doc = load(&path, Canvas { width: 1280, height: 720, fps: 60 }).unwrap();
        assert_eq!(doc.canvas.width, 1280, "the collection has to follow the core it is loaded on");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
