//! What a tag may be, and the one spelling each tag has.

use super::{Refusal, Tags};

/// The longest tag, in characters.
pub const TAG_MAX: usize = 40;
/// The most tags one file carries.
pub const TAGS_PER_FILE: usize = 30;

/// Tags a person typed: inner spaces made one and the ends trimmed, case
/// insensitive unique with the first spelling kept, and each spelled the way
/// the library already has it, so `easter` on one file and `Easter` on
/// another are one tag.
pub fn tag_list(tags: &Tags, words: &[String]) -> Result<Vec<String>, Refusal> {
    let mut out: Vec<String> = Vec::new();
    for raw in words {
        let tag = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        if tag.is_empty() {
            continue;
        }
        if tag.chars().count() > TAG_MAX || tag.chars().any(char::is_control) {
            return Err(Refusal::invalid(
                "tags",
                format!("the tag {tag:?} is longer than {TAG_MAX} characters or has a control character. Use a word or two, such as easter or choir."),
            ));
        }
        let tag = in_use(tags, &tag).unwrap_or(tag);
        if !out.iter().any(|t| same(t, &tag)) {
            out.push(tag);
        }
    }
    Ok(out)
}

/// Two spellings of one tag.
pub fn same(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

fn in_use(tags: &Tags, tag: &str) -> Option<String> {
    tags.files.values().flatten().find(|t| same(t, tag)).cloned()
}

/// A file's tags after a change: replaced, then added to, then taken from.
pub fn apply(have: Vec<String>, replace: Option<&[String]>, add: &[String], remove: &[String]) -> Vec<String> {
    let mut out = replace.map(<[String]>::to_vec).unwrap_or(have);
    for t in add {
        if !out.iter().any(|x| same(x, t)) {
            out.push(t.clone());
        }
    }
    out.retain(|t| !remove.iter().any(|r| same(r.trim(), t)));
    out
}

/// At most `TAGS_PER_FILE` on one file, or why not.
pub fn check_count(name: &str, tags: &[String]) -> Result<(), Refusal> {
    if tags.len() > TAGS_PER_FILE {
        return Err(Refusal::invalid(
            "tags",
            format!("{name} would have {} tags and a file keeps at most {TAGS_PER_FILE}. Take some off with remove_tags first.", tags.len()),
        ));
    }
    Ok(())
}

/// Tag each named file: `replace` them all when given, then `add`, then
/// `remove`. Answers each file's tags after, in the order named.
pub fn tag_files(
    dir: &std::path::Path,
    names: &[String],
    replace: Option<&[String]>,
    add: &[String],
    remove: &[String],
) -> Result<Vec<Vec<String>>, Refusal> {
    super::update(dir, |t| {
        let replace = replace.map(|r| tag_list(t, r)).transpose()?;
        let add = tag_list(t, add)?;
        let mut out = Vec::new();
        for name in names {
            let after = apply(t.of(name), replace.as_deref(), &add, remove);
            check_count(name, &after)?;
            t.files.insert(name.clone(), after.clone());
            out.push(after);
        }
        Ok(out)
    })
}
