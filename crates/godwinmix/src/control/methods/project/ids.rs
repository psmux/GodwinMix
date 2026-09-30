//! Renaming inside a scene collection, for a merge.
//!
//! Two things move when a project is merged into a mixer that already has
//! things of its own. Every scene and item gets a fresh id, because ids are
//! never shared between documents (the rule `preset.save` follows), and
//! references move with them: a `ref` to a scene and the keys of its
//! `overrides`. And a source renamed to dodge a clash (`cam-wide` becoming
//! `cam-wide-2`) is renamed wherever a scene draws it.

use godwinmix_core::scene::Id;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Give every id in `value` a fresh one, references included.
pub fn refresh(value: &mut Value) {
    let mut map = BTreeMap::new();
    collect(value, &mut map);
    rewrite(value, &map, &["id", "ref"], true);
}

fn collect(value: &Value, map: &mut BTreeMap<String, String>) {
    match value {
        Value::Object(o) => {
            if let Some(Value::String(id)) = o.get("id") {
                map.entry(id.clone()).or_insert_with(fresh);
            }
            o.values().for_each(|v| collect(v, map));
        }
        Value::Array(a) => a.iter().for_each(|v| collect(v, map)),
        _ => {}
    }
}

fn fresh() -> String {
    serde_json::to_value(Id::new()).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

/// Rename every string under one of `fields` found in `map`, and, when
/// `override_keys`, the keys of every `overrides` object.
fn rewrite(value: &mut Value, map: &BTreeMap<String, String>, fields: &[&str], override_keys: bool) {
    match value {
        Value::Object(o) => {
            for field in fields {
                if let Some(Value::String(s)) = o.get_mut(*field) {
                    if let Some(to) = map.get(s.as_str()) {
                        *s = to.clone();
                    }
                }
            }
            if override_keys {
                if let Some(Value::Object(overrides)) = o.get_mut("overrides") {
                    let moved: Map<String, Value> = std::mem::take(overrides)
                        .into_iter()
                        .map(|(k, v)| (map.get(&k).cloned().unwrap_or(k), v))
                        .collect();
                    *overrides = moved;
                }
            }
            o.values_mut().for_each(|v| rewrite(v, map, fields, override_keys));
        }
        Value::Array(a) => a.iter_mut().for_each(|v| rewrite(v, map, fields, override_keys)),
        _ => {}
    }
}

/// Rename sources wherever a collection names them: items that draw one,
/// and the labels in its `sources` table.
pub fn rename_sources(collection: &mut Value, renamed: &BTreeMap<String, String>) {
    if renamed.is_empty() {
        return;
    }
    if let Some(scenes) = collection.get_mut("scenes") {
        rewrite(scenes, renamed, &["source"], false);
    }
    if let Some(Value::Object(labels)) = collection.get_mut("sources") {
        let moved: Map<String, Value> = std::mem::take(labels)
            .into_iter()
            .map(|(k, v)| (renamed.get(&k).cloned().unwrap_or(k), v))
            .collect();
        *labels = moved;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ids_are_fresh_and_references_follow_them() {
        let mut scenes = json!([
            {"id": "a", "items": [{"id": "i1", "content": {"source": "cam"}}]},
            {"id": "b", "items": [{"id": "i2", "content": {"ref": "a", "overrides": {"i1": {"hidden": true}}}}]}
        ]);
        refresh(&mut scenes);
        let a = scenes[0]["id"].as_str().unwrap().to_string();
        let i1 = scenes[0]["items"][0]["id"].as_str().unwrap().to_string();
        assert_ne!(a, "a");
        assert_eq!(scenes[1]["items"][0]["content"]["ref"], a.as_str());
        assert!(scenes[1]["items"][0]["content"]["overrides"].get(&i1).is_some());
        assert_eq!(scenes[0]["items"][0]["content"]["source"], "cam", "a source is not an id to refresh");
    }

    #[test]
    fn a_renamed_source_is_renamed_where_scenes_draw_it() {
        let mut doc = json!({"scenes": [{"items": [{"content": {"source": "cam"}}]}], "sources": {"cam": {"name": "Camera"}}});
        let renamed = BTreeMap::from([("cam".to_string(), "cam-2".to_string())]);
        rename_sources(&mut doc, &renamed);
        assert_eq!(doc["scenes"][0]["items"][0]["content"]["source"], "cam-2");
        assert!(doc["sources"].get("cam-2").is_some());
    }
}
