//! Folders and tags in the media library.
//!
//! A folder is a real directory under the media folder, any depth, named by
//! its path relative to the media folder with `/` between names:
//! `Sermons/2026`. A file is named the same way: `Sermons/2026/easter.mp4`.
//! Tags are free words matched without regard to case, kept beside the
//! files because no file system tagging works on every platform.
//!
//! Like the gallery's, the request types are forgiving: a tag list may be an
//! array or one string with commas in it.

#[path = "media_folder.rs"]
mod folder;
pub use folder::*;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `media.list`. Every filter is optional and they all have to match.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MediaListRequest {
    /// Only the files in this folder: `Sermons/2026`. `""` is the top of the
    /// library. Left out, every file in the tree.
    #[serde(default)]
    pub folder: Option<String>,
    /// With `folder`: the files in its subfolders too, any depth.
    #[serde(default, alias = "recursive")]
    pub subfolders: bool,
    /// Only the files with this tag, or with every one of these tags:
    /// `"easter"`, `["easter", "choir"]` or `"easter, choir"`.
    #[serde(default, alias = "tags")]
    pub tag: Option<Value>,
    /// Words that each have to appear in the file's path or its tags.
    #[serde(default, alias = "search", alias = "q")]
    pub query: Option<String>,
}

/// `media.tag`: change the tags of one file or several. Only what is named
/// changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaTagRequest {
    /// One file, by its path in `media.list`.
    #[serde(default, alias = "id")]
    pub name: Option<String>,
    /// Several files at once.
    #[serde(default, alias = "files")]
    pub names: Vec<String>,
    /// Their tags from now on, replacing the ones they have. `[]` clears them.
    #[serde(default)]
    pub tags: Option<Value>,
    /// Tags to add, keeping the ones they have.
    #[serde(default, alias = "add", alias = "tag")]
    pub add_tags: Option<Value>,
    /// Tags to take off, keeping the rest.
    #[serde(default, alias = "untag", alias = "remove")]
    pub remove_tags: Option<Value>,
}

/// One file and its tags, as `media.tag` answers.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MediaTagged {
    pub name: String,
    pub tags: Vec<String>,
}

/// `media.move`: move files into a folder, rename one, or both. The files
/// move on disk; sources that use them are pointed at the new place.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaMoveRequest {
    /// One file, by its path in `media.list`.
    #[serde(default, alias = "id")]
    pub name: Option<String>,
    /// Several files at once.
    #[serde(default, alias = "files")]
    pub names: Vec<String>,
    /// The folder to move them into: `Sermons/2026`, or `""` for the top.
    /// Left out, they stay in their folder.
    #[serde(default, alias = "to", alias = "into")]
    pub folder: Option<String>,
    /// A new file name, keeping a clip or picture extension. One file only.
    #[serde(default, alias = "rename")]
    pub new_name: Option<String>,
    /// Move a file even while it is on programme. The source playing it
    /// is pointed at the new place, which restarts it on air.
    #[serde(default)]
    pub force: bool,
}

/// What moved, and what was changed to follow it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MediaMoved {
    /// Each move, old path to new, relative to the media folder.
    pub moved: Vec<MediaMove>,
    /// What was pointed at the new places, in words: `source intro`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub updated: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MediaMove {
    pub from: String,
    pub to: String,
}

/// Words from a tag field: an array of strings, or one string split at
/// commas and semicolons. Trimmed, empty ones dropped.
pub fn tag_words(v: Option<&Value>) -> Vec<String> {
    let raw: Vec<String> = match v {
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
        Some(Value::String(s)) => s.split([',', ';']).map(str::to_string).collect(),
        _ => Vec::new(),
    };
    raw.into_iter().map(|w| w.trim().to_string()).filter(|w| !w.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_tag_list_is_an_array_or_one_string_with_commas() {
        assert_eq!(tag_words(Some(&json!(["a", " b ", ""]))), ["a", "b"]);
        assert_eq!(tag_words(Some(&json!("easter, choir;; youth"))), ["easter", "choir", "youth"]);
        assert!(tag_words(None).is_empty());
    }

    #[test]
    fn the_requests_read_the_common_spellings_and_refuse_a_typo() {
        let r: MediaTagRequest = serde_json::from_value(json!({ "id": "a.mp4", "add": "x" })).unwrap();
        assert_eq!(r.name.as_deref(), Some("a.mp4"));
        assert_eq!(tag_words(r.add_tags.as_ref()), ["x"]);
        let m: MediaMoveRequest = serde_json::from_value(json!({ "names": ["a.mp4"], "to": "Ads" })).unwrap();
        assert_eq!(m.folder.as_deref(), Some("Ads"));
        assert!(serde_json::from_value::<MediaMoveRequest>(json!({ "name": "a", "fodler": "b" })).is_err());
    }
}
