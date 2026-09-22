//! Changing the operator's config file without destroying it.
//!
//! The config file is something a person wrote, with a comment on nearly every
//! line. Everything that writes to it (`preset.apply`, `plugin.settings.set`,
//! `config.set`) goes through `edit_file`, which parses it with `toml_edit`,
//! changes only the keys it was asked to change, and writes the rest back byte
//! for byte: comments, blank lines, key order and quoting included.

use std::path::Path;

use anyhow::{Context, Result};
use toml_edit::{DocumentMut, Item, Table, TableLike};

/// Read `path`, let `change` edit the document, and write it back atomically.
///
/// Nothing is written when `change` fails, and a crash mid write leaves the old
/// file, because the new one is written beside it and renamed over.
pub fn edit_file<T>(
    path: &Path,
    change: impl FnOnce(&mut DocumentMut) -> Result<T>,
) -> Result<T> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let mut doc: DocumentMut = text
        .parse()
        .with_context(|| format!("{} is not valid TOML", path.display()))?;
    let out = change(&mut doc)?;
    let body = doc.to_string();
    if body != text {
        write_atomic(path, body.as_bytes())?;
    }
    Ok(out)
}

/// Write then rename, so a crash mid write cannot leave half a file.
pub fn write_atomic(path: &Path, body: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("making {}", parent.display()))?;
        }
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, body).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))
}

/// Set one dotted key, `program.video_bitrate_kbps`, making the tables on the
/// way when the file does not have them yet. A key already there keeps its
/// place and its trailing comment; a new one goes at the end of its table.
pub fn set(doc: &mut DocumentMut, dotted: &str, value: &toml::Value) -> Result<()> {
    let (parents, leaf) = split(dotted)?;
    let table = table_at(doc.as_table_mut(), &parents, true)?
        .with_context(|| format!("no table for {dotted}"))?;
    put(table, leaf, to_item(value));
    Ok(())
}

/// Take one dotted key out of the file, so the built in default applies.
/// True when there was something to take out.
pub fn remove(doc: &mut DocumentMut, dotted: &str) -> Result<bool> {
    let (parents, leaf) = split(dotted)?;
    Ok(match table_at(doc.as_table_mut(), &parents, false)? {
        Some(table) => table.remove(leaf).is_some(),
        None => false,
    })
}

/// Make the table at `dotted` hold exactly `values`: keys in both keep their
/// place and comments, keys only in `values` are appended, and keys only in
/// the file are removed. What `plugin.settings.set` writes a plugin's table with.
pub fn replace_table(doc: &mut DocumentMut, dotted: &str, values: &toml::Table) -> Result<()> {
    let (parents, leaf) = split(dotted)?;
    let parent = table_at(doc.as_table_mut(), &parents, true)?
        .with_context(|| format!("no table for {dotted}"))?;
    match parent.get_mut(leaf).and_then(Item::as_table_like_mut) {
        Some(existing) => sync(existing, values),
        None => {
            parent.insert(leaf, to_item(&toml::Value::Table(values.clone())));
        }
    }
    Ok(())
}

/// Write one plugin's `[plugins.<name>]` table into the config file at `path`,
/// touching nothing else in it. What `plugin.settings.set` persists with.
pub fn write_plugin_settings(path: &Path, name: &str, settings: &toml::Table) -> Result<()> {
    anyhow::ensure!(
        !name.is_empty() && !name.contains('.'),
        "`{name}` is not a plugin name; a plugin name is a slug"
    );
    edit_file(path, |doc| replace_table(doc, &format!("plugins.{name}"), settings))
}

/// Bring a table in line with `values`, recursing into sub tables.
fn sync(table: &mut dyn TableLike, values: &toml::Table) {
    let gone: Vec<String> = table
        .iter()
        .map(|(k, _)| k.to_string())
        .filter(|k| !values.contains_key(k))
        .collect();
    for key in gone {
        table.remove(&key);
    }
    for (key, value) in values {
        match (table.get_mut(key).and_then(Item::as_table_like_mut), value) {
            (Some(inner), toml::Value::Table(sub)) => sync(inner, sub),
            _ => put(table, key, to_item(value)),
        }
    }
}

/// Insert or replace one key, keeping the decoration of a value already there
/// so `bitrate = 3000 # for the hall` keeps its comment when 3000 changes.
fn put(table: &mut dyn TableLike, key: &str, item: Item) {
    match (table.get_mut(key), item) {
        (Some(Item::Value(old)), Item::Value(mut new)) => {
            *new.decor_mut() = old.decor().clone();
            *old = new;
        }
        (_, item) => {
            table.insert(key, item);
        }
    }
}

/// Walk to the table at `path`, making each step when `create` is set.
fn table_at<'a>(
    root: &'a mut Table,
    path: &[&str],
    create: bool,
) -> Result<Option<&'a mut dyn TableLike>> {
    let mut here: &mut dyn TableLike = root;
    for (n, step) in path.iter().enumerate() {
        if here.get(step).is_none() {
            if !create {
                return Ok(None);
            }
            let mut fresh = Table::new();
            // A table holding only other tables prints no header of its own.
            fresh.set_implicit(n + 1 < path.len());
            here.insert(step, Item::Table(fresh));
        }
        let Some(next) = here.get_mut(step).and_then(Item::as_table_like_mut) else {
            anyhow::bail!("`{}` in the config is a value, not a table", path[..=n].join("."));
        };
        here = next;
    }
    Ok(Some(here))
}

fn split(dotted: &str) -> Result<(Vec<&str>, &str)> {
    let mut parts: Vec<&str> = dotted.split('.').collect();
    anyhow::ensure!(parts.iter().all(|p| !p.is_empty()), "`{dotted}` is not a dotted key");
    let leaf = parts.pop().unwrap_or_default();
    Ok((parts, leaf))
}

/// A `toml` value as a `toml_edit` item: tables as `[section]` tables, and
/// everything inside a value (an array, a table in an array) inline.
pub fn to_item(value: &toml::Value) -> Item {
    match value {
        toml::Value::Table(t) => {
            let mut table = Table::new();
            for (k, v) in t {
                table.insert(k, to_item(v));
            }
            Item::Table(table)
        }
        other => Item::Value(to_value(other)),
    }
}

fn to_value(value: &toml::Value) -> toml_edit::Value {
    match value {
        toml::Value::String(s) => s.as_str().into(),
        toml::Value::Integer(i) => (*i).into(),
        toml::Value::Float(f) => (*f).into(),
        toml::Value::Boolean(b) => (*b).into(),
        toml::Value::Datetime(d) => d
            .to_string()
            .parse::<toml_edit::Datetime>()
            .map(Into::into)
            .unwrap_or_else(|_| d.to_string().into()),
        toml::Value::Array(items) => {
            toml_edit::Value::Array(items.iter().map(to_value).collect())
        }
        toml::Value::Table(t) => toml_edit::Value::InlineTable(
            t.iter().map(|(k, v)| (k.as_str(), to_value(v))).collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "\
# My mixer. Do not lose this line.
[program]
# the hall's uplink is slow
video_bitrate_kbps = 3000 # was 6000
audio_bitrate_kbps = 160

[plugins.ndi]
# the NDI group we use
group = \"hall\"
extra = 1
";

    fn doc() -> DocumentMut {
        FILE.parse().unwrap()
    }

    #[test]
    fn setting_a_key_keeps_every_comment_and_the_one_on_its_line() {
        let mut d = doc();
        set(&mut d, "program.video_bitrate_kbps", &toml::Value::Integer(4500)).unwrap();
        let out = d.to_string();
        assert!(out.contains("video_bitrate_kbps = 4500 # was 6000"), "{out}");
        assert!(out.contains("# My mixer. Do not lose this line."), "{out}");
        assert!(out.contains("# the hall's uplink is slow"), "{out}");
    }

    #[test]
    fn a_key_in_a_section_the_file_lacks_makes_the_section() {
        let mut d = doc();
        set(&mut d, "security.allow_exec_sources", &toml::Value::Boolean(true)).unwrap();
        let back: toml::Table = toml::from_str(&d.to_string()).unwrap();
        assert_eq!(back["security"]["allow_exec_sources"].as_bool(), Some(true));
        assert!(d.to_string().starts_with("# My mixer."));
    }

    #[test]
    fn removing_a_key_leaves_its_neighbours() {
        let mut d = doc();
        assert!(remove(&mut d, "program.audio_bitrate_kbps").unwrap());
        assert!(!remove(&mut d, "canvas.width").unwrap());
        let out = d.to_string();
        assert!(!out.contains("audio_bitrate_kbps"));
        assert!(out.contains("video_bitrate_kbps = 3000 # was 6000"));
    }

    #[test]
    fn replacing_a_plugin_table_keeps_its_comment_and_drops_what_went() {
        let mut d = doc();
        let mut values = toml::Table::new();
        values.insert("group".into(), "stage".into());
        values.insert("fresh".into(), toml::Value::Boolean(true));
        replace_table(&mut d, "plugins.ndi", &values).unwrap();
        let out = d.to_string();
        assert!(out.contains("# the NDI group we use\ngroup = \"stage\""), "{out}");
        assert!(!out.contains("extra"), "{out}");
        assert!(out.contains("fresh = true"), "{out}");
    }

    #[test]
    fn a_value_where_a_table_should_be_is_refused_by_name() {
        let mut d: DocumentMut = "program = 3\n".parse().unwrap();
        let e = set(&mut d, "program.video_bitrate_kbps", &1.into()).unwrap_err();
        assert!(format!("{e}").contains("`program`"), "{e}");
    }
}
