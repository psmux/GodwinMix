//! The library's folders and tags, carried in a project and put back by an
//! import. The folders are the clips' own paths plus any empty ones; the
//! tags go onto clips that are here after the import.

use super::bundle::Bundle;
use godwinmix_core::media_org as org;
use std::path::Path;

/// What an import lays out in the media library.
#[derive(Debug, Default)]
pub struct Layout {
    pub folders: Vec<String>,
    /// A clip's path and the tags the file gave it.
    pub tags: Vec<(String, Vec<String>)>,
}

/// Worked out before anything is written. A clip whose name is taken here by
/// a different file gets no tags: they were never about that file.
pub fn layout(dir: &Path, bundle: &Bundle) -> Layout {
    let tags = bundle
        .media
        .iter()
        .filter(|e| !e.tags.is_empty() && matches!(super::media::action(dir, e), "add" | "keep"))
        .map(|e| (e.name.clone(), e.tags.clone()))
        .collect();
    Layout { folders: bundle.media_folders.clone(), tags }
}

/// Make the folders and give the tags, adding to any a clip has here.
/// Answers what could not be done, in words.
pub fn lay_out(dir: &Path, layout: &Layout) -> Vec<String> {
    let mut failed = Vec::new();
    for f in &layout.folders {
        if let Err(r) = org::make_folder(dir, f) {
            failed.push(format!("the media folder {f} was not made: {}", r.message));
        }
    }
    for (name, tags) in &layout.tags {
        if !dir.join(name).is_file() {
            continue;
        }
        if let Err(r) = org::tag_files(dir, std::slice::from_ref(name), None, tags, &[]) {
            failed.push(format!("{name} was not tagged: {}", r.message));
        }
    }
    failed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::methods::project::bundle::MediaEntry;

    #[test]
    fn an_import_makes_the_folders_and_tags_the_clips_that_are_here() {
        let dir = std::env::temp_dir().join(format!("gmx-project-layout-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Sermons")).unwrap();
        std::fs::write(dir.join("Sermons/a.mp4"), b"abc").unwrap();
        std::fs::write(dir.join("b.mp4"), b"different").unwrap();
        let mut bundle = Bundle::new("x", false);
        bundle.media_folders = vec!["Sermons".into(), "Sermons/2026".into(), "Empty".into()];
        let entry = |name: &str, size: u64| MediaEntry { name: name.into(), size, data: None, tags: vec!["easter".into()] };
        bundle.media = vec![entry("Sermons/a.mp4", 3), entry("b.mp4", 3)];
        let plan = layout(&dir, &bundle);
        assert_eq!(plan.tags.len(), 1, "b.mp4 here is a different file and keeps its own tags");
        assert!(lay_out(&dir, &plan).is_empty());
        assert!(dir.join("Sermons/2026").is_dir() && dir.join("Empty").is_dir());
        assert_eq!(org::load(&dir).of("Sermons/a.mp4"), ["easter"]);
        assert!(org::load(&dir).of("b.mp4").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
