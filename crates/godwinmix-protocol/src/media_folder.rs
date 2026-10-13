//! The request and answer bodies of `media.folder.*`.

use serde::{Deserialize, Serialize};

/// One folder, as `media.folder.list` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MediaFolderCount {
    /// `Sermons/2026`.
    pub path: String,
    /// Files right in it.
    pub files: usize,
    /// Files anywhere under it, its own included.
    pub total: usize,
}

/// One tag and how many files have it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MediaCount {
    pub name: String,
    pub count: usize,
}

/// The answer to `media.folder.list`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MediaFolders {
    /// Every folder, sorted by path, empty ones included.
    pub folders: Vec<MediaFolderCount>,
    /// Every tag in use, the most used first.
    pub tags: Vec<MediaCount>,
}

/// `media.folder.add`: make a folder, and any above it that are missing.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MediaFolderAddRequest {
    /// `Sermons`, or `Sermons/2026` to make one inside another.
    #[serde(default, alias = "path", alias = "folder")]
    pub name: String,
}

/// `media.folder.move`: rename a folder, move it into another, or both.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaFolderMoveRequest {
    /// The folder as it is now: `Sermons/2026`.
    #[serde(default, alias = "path", alias = "folder", alias = "id")]
    pub name: String,
    /// The folder to put it in, `""` for the top. Left out, it stays where
    /// it is.
    #[serde(default, alias = "parent")]
    pub into: Option<String>,
    /// Its new name. Left out, it keeps its name.
    #[serde(default, alias = "new_name", alias = "rename")]
    pub to: Option<String>,
    /// Move it even while a file in it is on programme.
    #[serde(default)]
    pub force: bool,
}

/// `media.folder.remove`: what happens to what is inside is asked for, never
/// assumed. No media file is ever deleted.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MediaFolderRemoveRequest {
    #[serde(default, alias = "path", alias = "folder", alias = "id")]
    pub name: String,
    /// `up` moves its files and folders into the folder above it; `top`
    /// moves them to the top of the library.
    #[serde(default)]
    pub contents: Option<String>,
    /// Remove it even while a file in it is on programme.
    #[serde(default)]
    pub force: bool,
}
