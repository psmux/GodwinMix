//! Narrowing a listing by folder, tag and words, and counting what is used.

use super::Labels;
use crate::media::MediaItem;
use godwinmix_protocol::media::{MediaCount, MediaFolders};

/// What `media.list` was asked to keep. Empty keeps everything.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub folder: Option<String>,
    pub tags: Vec<String>,
    pub query: Option<String>,
}

impl Filter {
    pub fn is_empty(&self) -> bool {
        self.folder.is_none() && self.tags.is_empty() && self.query.is_none()
    }

    /// Keep the items that match every part of the filter.
    pub fn apply(&self, items: &mut Vec<MediaItem>) {
        let folder = self.folder.as_deref().map(str::trim).filter(|f| !f.is_empty() && !f.eq_ignore_ascii_case("all")).map(str::to_lowercase);
        let tags: Vec<String> = self.tags.iter().map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty()).collect();
        let words: Vec<String> = self.query.as_deref().unwrap_or("").split_whitespace().map(str::to_lowercase).collect();
        items.retain(|i| {
            let have: Vec<String> = i.tags.iter().map(|t| t.to_lowercase()).collect();
            let in_folder = folder.as_ref().is_none_or(|f| i.folder.as_ref().is_some_and(|x| x.to_lowercase() == *f));
            let tagged = tags.iter().all(|t| have.contains(t));
            let text = format!("{} {}", i.name.to_lowercase(), have.join(" "));
            in_folder && tagged && words.iter().all(|w| text.contains(w.as_str()))
        });
    }
}

/// Every folder with how many files are in it, and every tag in use with how
/// many files carry it, the most used first and then by name.
pub fn counts(labels: &Labels) -> MediaFolders {
    let folders = labels
        .folders
        .iter()
        .map(|f| MediaCount { name: f.clone(), count: labels.files.values().filter(|l| l.folder.as_ref() == Some(f)).count() })
        .collect();
    let mut tags: Vec<MediaCount> = Vec::new();
    for tag in labels.files.values().flat_map(|l| l.tags.iter()) {
        match tags.iter_mut().find(|c| c.name.to_lowercase() == tag.to_lowercase()) {
            Some(c) => c.count += 1,
            None => tags.push(MediaCount { name: tag.clone(), count: 1 }),
        }
    }
    tags.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    MediaFolders { folders, tags }
}
