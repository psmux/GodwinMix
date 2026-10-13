//! What a folder name and a tag may be.

use super::{Labels, Refusal};

/// The longest folder name, in characters.
pub const FOLDER_MAX: usize = 60;
/// The longest tag, in characters.
pub const TAG_MAX: usize = 40;
/// The most tags one file carries.
pub const TAGS_PER_FILE: usize = 30;

/// Inner runs of spaces made one, the ends trimmed.
fn tidy(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A folder name a person typed, made tidy, or why it cannot be one. One
/// level: a slash would read as a folder inside a folder, which there is not.
/// "All" is what the panel calls every file, so no folder can be called that.
pub fn folder_name(raw: &str) -> Result<String, Refusal> {
    let name = tidy(raw);
    let refuse = |why: &str| Refusal::invalid("folder", format!("{why} Pick a short plain name such as Sermons, Ads or Backgrounds."));
    if name.is_empty() {
        return Err(refuse("a folder needs a name."));
    }
    if name.chars().count() > FOLDER_MAX {
        return Err(refuse(&format!("{name:?} is longer than {FOLDER_MAX} characters.")));
    }
    if name.contains(['/', '\\']) || name.chars().any(char::is_control) {
        return Err(refuse(&format!("{name:?} has a slash or a control character in it, and folders are one level.")));
    }
    if name.eq_ignore_ascii_case("all") {
        return Err(refuse("All is every file already, so no folder can be called that."));
    }
    Ok(name)
}

/// Tags a person typed: tidied, case insensitive unique with the first
/// spelling kept, and each spelled the way the library already has it, so
/// `easter` on one file and `Easter` on another are one tag.
pub fn tag_list(labels: &Labels, words: &[String]) -> Result<Vec<String>, Refusal> {
    let mut out: Vec<String> = Vec::new();
    for raw in words {
        let tag = tidy(raw);
        if tag.is_empty() {
            continue;
        }
        if tag.chars().count() > TAG_MAX || tag.chars().any(char::is_control) {
            return Err(Refusal::invalid(
                "tags",
                format!("the tag {tag:?} is longer than {TAG_MAX} characters or has a control character. Use a word or two, such as easter or choir."),
            ));
        }
        let tag = in_use(labels, &tag).unwrap_or(tag);
        if !out.iter().any(|t| t.to_lowercase() == tag.to_lowercase()) {
            out.push(tag);
        }
    }
    Ok(out)
}

/// The library's own spelling of a tag, when some file has it.
fn in_use(labels: &Labels, tag: &str) -> Option<String> {
    let low = tag.to_lowercase();
    labels.files.values().flat_map(|l| l.tags.iter()).find(|t| t.to_lowercase() == low).cloned()
}

/// The folder of that name, in the case it was made, when there is one.
pub fn find_folder<'a>(labels: &'a Labels, name: &str) -> Option<&'a String> {
    let low = tidy(name).to_lowercase();
    labels.folders.iter().find(|f| f.to_lowercase() == low)
}

/// Keep at most `TAGS_PER_FILE`, or say why not.
pub fn check_count(name: &str, tags: &[String]) -> Result<(), Refusal> {
    if tags.len() > TAGS_PER_FILE {
        return Err(Refusal::invalid(
            "tags",
            format!("{name} would have {} tags and a file keeps at most {TAGS_PER_FILE}. Take some off with remove_tags first.", tags.len()),
        ));
    }
    Ok(())
}
