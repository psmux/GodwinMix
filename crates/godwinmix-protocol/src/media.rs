//! Folders and tags in the media library.
//!
//! A file sits in at most one folder, a plain name one level deep ("Sermons",
//! "Ads"), and carries any number of tags, free words matched without regard
//! to case. A file in no folder is in All only. None of this touches the
//! files: it is kept beside them, and a file renamed or deleted by hand loses
//! its folder and tags the next time the library is read.
//!
//! Like the gallery's, the request types are forgiving: a tag list may be an
//! array or one string with commas in it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A file's folder and tags, as `media.list` gives them on each item.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MediaLabel {
    /// The folder it is in. Absent when it is in All only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// Its tags, in the case each was first typed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl MediaLabel {
    pub fn is_empty(&self) -> bool {
        self.folder.is_none() && self.tags.is_empty()
    }
}

/// `media.list`. Every filter is optional and they all have to match.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MediaListRequest {
    /// Only the files in this folder. Left out, empty or "All" lists every
    /// file.
    #[serde(default)]
    pub folder: Option<String>,
    /// Only the files with this tag, or with every one of these tags:
    /// `"easter"`, `["easter", "choir"]` or `"easter, choir"`.
    #[serde(default, alias = "tags")]
    pub tag: Option<Value>,
    /// Words that each have to appear in the file's name or its tags.
    #[serde(default, alias = "search", alias = "q")]
    pub query: Option<String>,
}

/// `media.set`: move files to a folder and change their tags. Only what is
/// named changes: leave `folder` out and the files stay where they are.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaSetRequest {
    /// One file, by its name in `media.list`.
    #[serde(default, alias = "id")]
    pub name: Option<String>,
    /// Several files at once, by name.
    #[serde(default, alias = "files")]
    pub names: Vec<String>,
    /// The folder to put them in. A folder that does not exist yet is made.
    /// `""` or `"All"` takes them out of their folder.
    #[serde(default)]
    pub folder: Option<String>,
    /// Their tags from now on, replacing the ones they have. `[]` clears
    /// them.
    #[serde(default)]
    pub tags: Option<Value>,
    /// Tags to add, keeping the ones they have.
    #[serde(default, alias = "add", alias = "tag")]
    pub add_tags: Option<Value>,
    /// Tags to take off, keeping the rest.
    #[serde(default, alias = "untag")]
    pub remove_tags: Option<Value>,
}

/// One file's name with its folder and tags, as `media.set` answers.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MediaLabelled {
    pub name: String,
    #[serde(flatten)]
    pub label: MediaLabel,
}

/// The answer to `media.set`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MediaSetAnswer {
    pub files: Vec<MediaLabelled>,
    /// A folder this call made, when it named one that was not there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub made_folder: Option<String>,
}

/// One folder or tag and how many files have it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MediaCount {
    pub name: String,
    pub count: usize,
}

/// The answer to `media.folder.list`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MediaFolders {
    /// Every folder, in the order they were made, empty ones included.
    pub folders: Vec<MediaCount>,
    /// Every tag in use, the most used first.
    pub tags: Vec<MediaCount>,
}

/// `media.folder.add` and `media.folder.remove`: one folder by name.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MediaFolderRequest {
    #[serde(default, alias = "folder", alias = "id")]
    pub name: String,
}

/// `media.folder.rename`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MediaFolderRenameRequest {
    /// The folder as it is called now.
    #[serde(default, alias = "folder", alias = "from")]
    pub name: String,
    /// What to call it.
    #[serde(default, alias = "new_name")]
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
    fn a_set_request_reads_the_common_spellings_and_refuses_a_typo() {
        let r: MediaSetRequest = serde_json::from_value(json!({ "id": "a.mp4", "add": "x" })).unwrap();
        assert_eq!(r.name.as_deref(), Some("a.mp4"));
        assert_eq!(tag_words(r.add_tags.as_ref()), ["x"]);
        assert!(serde_json::from_value::<MediaSetRequest>(json!({ "name": "a", "fodler": "b" })).is_err());
    }
}
