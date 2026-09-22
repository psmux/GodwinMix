//! Merging a preset's config into the operator's, in place.
//!
//! The operator's file is edited, not rewritten: every comment, blank line and
//! key they wrote stays where it was. What the preset brings is added beside
//! it, with the preset's own comments, so a section it filled in arrives
//! documented. The file as it was is still kept as `.bak`, because a merge is
//! a big change to make to somebody's file in one step.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, TableLike, Value};

use super::plan::Plan;
use crate::config::edit;

/// The config file. Returns the backup path when one was made.
pub fn write_config(plan: &Plan, preset_config: &str) -> Result<Option<PathBuf>> {
    let path = crate::config::path_in_force(&plan.config_path);
    if !path.exists() {
        // Nothing to merge into: the preset's own file, comments and all.
        edit::write_atomic(&path, preset_config.as_bytes())?;
        return Ok(None);
    }
    let preset: DocumentMut =
        preset_config.parse().context("the preset's config is not valid TOML")?;
    let backup = keep_original(&path)?;
    edit::edit_file(&path, |doc| {
        merge(doc.as_table_mut(), preset.as_table(), plan.force);
        if !plan.keep_sources {
            append_by_id(doc, &preset, "sources");
            append_by_id(doc, &preset, "outputs");
        }
        Ok(())
    })?;
    Ok(Some(backup))
}

fn keep_original(path: &Path) -> Result<PathBuf> {
    let backup = path.with_extension("toml.bak");
    std::fs::copy(path, &backup).with_context(|| format!("writing {}", backup.display()))?;
    Ok(backup)
}

/// Preset values fill gaps; the operator's own win unless `force`.
fn merge(current: &mut dyn TableLike, preset: &dyn TableLike, force: bool) {
    for (key, theirs) in preset.iter() {
        if matches!(key, "sources" | "outputs") {
            continue;
        }
        let mine_is_table = current.get(key).map(Item::is_table_like);
        match mine_is_table {
            Some(true) if theirs.is_table_like() => {
                if let (Some(mine), Some(theirs)) = (
                    current.get_mut(key).and_then(Item::as_table_like_mut),
                    theirs.as_table_like(),
                ) {
                    merge(mine, theirs, force);
                }
            }
            Some(_) if !force => {}
            Some(_) => match (current.get_mut(key), theirs.as_value()) {
                // Force over a value keeps the comment on the operator's line.
                (Some(Item::Value(old)), Some(new)) => {
                    let mut new = new.clone();
                    *new.decor_mut() = old.decor().clone();
                    *old = new;
                }
                _ => {
                    current.insert(key, detached(theirs.clone()));
                }
            },
            None => {
                current.insert(key, detached(theirs.clone()));
            }
        }
    }
}

/// Append the preset's entries whose id is not already in the operator's list.
fn append_by_id(doc: &mut DocumentMut, preset: &DocumentMut, key: &str) {
    let have: Vec<String> = doc.get(key).map(entries).unwrap_or_default().iter().filter_map(id_of).collect();
    let fresh: Vec<Table> = preset
        .get(key)
        .map(entries)
        .unwrap_or_default()
        .into_iter()
        .filter(|t| id_of(t).is_some_and(|id| !have.contains(&id)))
        .map(|mut t| {
            clear_positions(&mut t);
            t
        })
        .collect();
    if fresh.is_empty() {
        return;
    }
    match doc.get_mut(key) {
        None => {
            let mut list = ArrayOfTables::new();
            fresh.into_iter().for_each(|t| list.push(t));
            doc.insert(key, Item::ArrayOfTables(list));
        }
        Some(Item::ArrayOfTables(list)) => fresh.into_iter().for_each(|t| list.push(t)),
        Some(Item::Value(Value::Array(list))) => {
            fresh.into_iter().for_each(|t| list.push(t.into_inline_table()))
        }
        Some(_) => tracing::warn!("`{key}` in the config is not a list; the preset's were not added"),
    }
}

/// The entries of `[[sources]]` or `sources = [ .. ]`, as tables.
fn entries(item: &Item) -> Vec<Table> {
    match item {
        Item::ArrayOfTables(list) => list.iter().cloned().collect(),
        Item::Value(Value::Array(list)) => list
            .iter()
            .filter_map(Value::as_inline_table)
            .map(|t| t.clone().into_table())
            .collect(),
        _ => Vec::new(),
    }
}

fn id_of(table: &Table) -> Option<String> {
    table.get("id").and_then(Item::as_str).map(str::to_string)
}

/// A table copied from the preset keeps the preset's idea of where it sat in
/// the preset's file. Clearing it puts it after the operator's own tables.
fn detached(mut item: Item) -> Item {
    match &mut item {
        Item::Table(t) => clear_positions(t),
        Item::ArrayOfTables(list) => list.iter_mut().for_each(clear_positions),
        _ => {}
    }
    item
}

fn clear_positions(table: &mut Table) {
    table.set_position(None);
    for (_, item) in table.iter_mut() {
        match item {
            Item::Table(t) => clear_positions(t),
            Item::ArrayOfTables(list) => list.iter_mut().for_each(clear_positions),
            _ => {}
        }
    }
}
