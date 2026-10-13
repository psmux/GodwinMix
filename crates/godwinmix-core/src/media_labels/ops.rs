//! Moving files between folders, tagging them, and the folders themselves.

use super::rules::{check_count, find_folder, folder_name, tag_list};
use super::Labels;
use godwinmix_protocol::media::MediaLabel;
use std::path::Path;

/// Why a change was refused, with what a caller needs to put it right.
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    pub message: String,
    /// The param at fault: `folder`, `tags`, `to`.
    pub field: &'static str,
    /// For a folder that is not there: the folders that are.
    pub valid: Option<Vec<String>>,
}

impl Refusal {
    pub fn invalid(field: &'static str, message: String) -> Self {
        Self { message, field, valid: None }
    }

    fn no_folder(labels: &Labels, name: &str) -> Self {
        let have = if labels.folders.is_empty() { "There are no folders yet".to_string() } else { format!("The folders are {}", labels.folders.join(", ")) };
        Self {
            message: format!("there is no folder {name:?}. {have}. Call media.folder.list to see them, or media.folder.add to make it."),
            field: "folder",
            valid: Some(labels.folders.clone()),
        }
    }

    pub fn io(dir: &Path, e: std::io::Error) -> Self {
        Self::invalid(
            "folder",
            format!("the folders and tags could not be saved in {}: {e}. Check the media folder can be written to, then try again.", dir.display()),
        )
    }
}

/// What one `media.set` does to each file. `None` leaves that part alone.
#[derive(Debug, Clone, Default)]
pub struct Change {
    /// `Some("")` takes the file out of its folder.
    pub folder: Option<String>,
    pub tags: Option<Vec<String>>,
    pub add: Vec<String>,
    pub remove: Vec<String>,
}

/// The folder a change names, made when it is not there yet. `None` inside
/// means no folder. The second value is true when it was made now.
fn target(labels: &mut Labels, raw: &str) -> Result<(Option<String>, bool), Refusal> {
    let t = raw.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("all") {
        return Ok((None, false));
    }
    let (name, made) = add_folder(labels, t)?;
    Ok((Some(name), made))
}

/// Apply one change to each named file. Answers each file's labels after it
/// and the folder made, if one was.
pub fn set(labels: &mut Labels, names: &[String], change: &Change) -> Result<(Vec<MediaLabel>, Option<String>), Refusal> {
    let mut made = None;
    let folder = match &change.folder {
        Some(raw) => {
            let (f, new) = target(labels, raw)?;
            if new {
                made = f.clone();
            }
            Some(f)
        }
        None => None,
    };
    let replace = change.tags.as_ref().map(|t| tag_list(labels, t)).transpose()?;
    let add = tag_list(labels, &change.add)?;
    let drop: Vec<String> = change.remove.iter().map(|t| t.trim().to_lowercase()).collect();
    let mut out = Vec::new();
    for name in names {
        let mut label = labels.of(name);
        if let Some(f) = &folder {
            label.folder = f.clone();
        }
        if let Some(r) = &replace {
            label.tags = r.clone();
        }
        for t in &add {
            if !label.tags.iter().any(|x| x.to_lowercase() == t.to_lowercase()) {
                label.tags.push(t.clone());
            }
        }
        label.tags.retain(|t| !drop.contains(&t.to_lowercase()));
        check_count(name, &label.tags)?;
        labels.files.insert(name.clone(), label.clone());
        out.push(label);
    }
    Ok((out, made))
}

/// Make a folder. The name it has, and true when it is new; asking for one
/// that is there already, in any case, answers that one.
pub fn add_folder(labels: &mut Labels, raw: &str) -> Result<(String, bool), Refusal> {
    let name = folder_name(raw)?;
    if let Some(have) = find_folder(labels, &name) {
        return Ok((have.clone(), false));
    }
    labels.folders.push(name.clone());
    Ok((name, true))
}

/// Rename a folder, taking its files with it. Answers the files moved.
pub fn rename_folder(labels: &mut Labels, from: &str, to: &str) -> Result<Vec<String>, Refusal> {
    let old = find_folder(labels, from).cloned().ok_or_else(|| Refusal::no_folder(labels, from))?;
    let new = folder_name(to).map_err(|r| Refusal { field: "to", ..r })?;
    if let Some(other) = find_folder(labels, &new).filter(|f| **f != old) {
        return Err(Refusal::invalid(
            "to",
            format!("there is already a folder called {other:?}. Pick another name, or move the files across with media.set and remove {old:?}."),
        ));
    }
    for f in labels.folders.iter_mut().filter(|f| **f == old) {
        *f = new.clone();
    }
    Ok(move_files(labels, &old, Some(&new)))
}

/// Remove a folder. Its files stay in the library, in All. Answers them.
pub fn remove_folder(labels: &mut Labels, raw: &str) -> Result<Vec<String>, Refusal> {
    let old = find_folder(labels, raw).cloned().ok_or_else(|| Refusal::no_folder(labels, raw))?;
    labels.folders.retain(|f| *f != old);
    Ok(move_files(labels, &old, None))
}

fn move_files(labels: &mut Labels, from: &str, to: Option<&str>) -> Vec<String> {
    let mut moved = Vec::new();
    for (name, label) in labels.files.iter_mut() {
        if label.folder.as_deref() == Some(from) {
            label.folder = to.map(str::to_string);
            moved.push(name.clone());
        }
    }
    moved
}
