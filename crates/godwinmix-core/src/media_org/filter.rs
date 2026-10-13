//! Narrowing a listing by folder, tag and words, and counting what is used.

use super::paths::parent_of;
use super::tag_rules::same;
use super::Tags;
use crate::media::MediaItem;
use godwinmix_protocol::media::{MediaCount, MediaFolderCount};

/// What `media.list` was asked to keep. Empty keeps everything.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// A folder, `Sermons/2026`. `""` is the top of the library.
    pub folder: Option<String>,
    /// With `folder`: its subfolders' files as well.
    pub subfolders: bool,
    pub tags: Vec<String>,
    pub query: Option<String>,
}

impl Filter {
    /// Whether one file, by its path in the library, is in the folder asked
    /// for.
    fn in_folder(&self, name: &str) -> bool {
        let Some(f) = self.folder.as_deref() else { return true };
        let here = parent_of(name);
        let low = |s: &str| s.to_lowercase();
        low(here) == low(f) || (self.subfolders && (f.is_empty() || low(here).starts_with(&format!("{}/", low(f)))))
    }

    /// Keep the items that match every part of the filter.
    pub fn apply(&self, items: &mut Vec<MediaItem>) {
        let words: Vec<String> = self.query.as_deref().unwrap_or("").split_whitespace().map(str::to_lowercase).collect();
        items.retain(|i| {
            let tagged = self.tags.iter().all(|t| i.tags.iter().any(|x| same(x, t.trim())));
            let text = format!("{} {}", i.name.to_lowercase(), i.tags.join(" ").to_lowercase());
            self.in_folder(&i.name) && tagged && words.iter().all(|w| text.contains(w.as_str()))
        });
    }
}

/// Each folder with the files right in it and the files anywhere under it.
pub fn folder_counts(folders: &[String], names: &[String]) -> Vec<MediaFolderCount> {
    folders
        .iter()
        .map(|f| {
            let prefix = format!("{f}/");
            MediaFolderCount {
                path: f.clone(),
                files: names.iter().filter(|n| parent_of(n) == f).count(),
                total: names.iter().filter(|n| n.starts_with(&prefix)).count(),
            }
        })
        .collect()
}

/// Every tag in use with how many files carry it, the most used first and
/// then by name.
pub fn tag_counts(tags: &Tags) -> Vec<MediaCount> {
    let mut out: Vec<MediaCount> = Vec::new();
    for tag in tags.files.values().flatten() {
        match out.iter_mut().find(|c| same(&c.name, tag)) {
            Some(c) => c.count += 1,
            None => out.push(MediaCount { name: tag.clone(), count: 1 }),
        }
    }
    out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out
}
