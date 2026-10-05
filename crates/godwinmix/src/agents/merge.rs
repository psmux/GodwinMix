//! Putting one entry into somebody else's config file and leaving the rest
//! of it alone.
//!
//! The file belongs to the agent tool and to the person, so only the one
//! entry named `godwinmix` changes. A JSON file keeps every other key; a TOML
//! file keeps its comments and its order too, through `toml_edit`. A file
//! that cannot be read as what it should be is refused with the entry to add
//! by hand, never rewritten.

use serde_json::{Map, Value};

/// The server's name in every tool's config.
pub const NAME: &str = "godwinmix";

/// What a write does to a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    /// There was no file.
    Create,
    /// The file is there and gets the entry beside what it has.
    Merge,
    /// The file has a `godwinmix` entry that differs, and it is replaced.
    Update,
    /// The file already says exactly this.
    Unchanged,
}

/// The new text of a JSON config with the entry under `key`.
pub fn json(existing: Option<&str>, key: &str, entry: &Value, seed: Value) -> Result<(String, Action), String> {
    let (mut doc, mut action) = match existing.map(str::trim) {
        None | Some("") => (seed, Action::Create),
        Some(text) => (
            serde_json::from_str::<Value>(text).map_err(|e| {
                format!("it is not plain JSON ({e}); comments are the usual reason. Add the entry by hand")
            })?,
            Action::Merge,
        ),
    };
    let Some(root) = doc.as_object_mut() else {
        return Err("its top level is not a JSON object. Add the entry by hand".into());
    };
    let servers = root.entry(key.to_string()).or_insert_with(|| Value::Object(Map::new()));
    let Some(servers) = servers.as_object_mut() else {
        return Err(format!("its `{key}` is not an object. Add the entry by hand"));
    };
    match servers.get(NAME) {
        Some(old) if old == entry => action = Action::Unchanged,
        Some(_) => action = Action::Update,
        None => {}
    }
    servers.insert(NAME.to_string(), entry.clone());
    let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())? + "\n";
    Ok((text, action))
}

/// `item` added to the list under `key`, once. For opencode's `instructions`.
pub fn json_list_add(text: &str, key: &str, item: &str) -> Result<(String, bool), String> {
    let mut doc: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let list = doc
        .as_object_mut()
        .ok_or("its top level is not a JSON object")?
        .entry(key.to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(list) = list.as_array_mut() else {
        return Err(format!("its `{key}` is not a list. Add {item:?} to it by hand"));
    };
    if list.iter().any(|v| v.as_str() == Some(item)) {
        return Ok((text.to_string(), false));
    }
    list.push(Value::String(item.to_string()));
    Ok((serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())? + "\n", true))
}

/// The new text of a Codex `config.toml` with `[mcp_servers.godwinmix]`.
pub fn toml(existing: Option<&str>, entry: &Value) -> Result<(String, Action), String> {
    use toml_edit::{value, Array, DocumentMut, InlineTable, Item, Table};
    let mut doc: DocumentMut = existing
        .unwrap_or_default()
        .parse()
        .map_err(|e| format!("it is not TOML ({e}). Add the entry by hand"))?;
    let mut table = Table::new();
    table["command"] = value(entry["command"].as_str().unwrap_or_default());
    let mut args = Array::new();
    for a in entry["args"].as_array().into_iter().flatten() {
        args.push(a.as_str().unwrap_or_default());
    }
    table["args"] = value(args);
    if let Some(env) = entry.get("env").and_then(Value::as_object) {
        let mut inline = InlineTable::new();
        for (k, v) in env {
            inline.insert(k, v.as_str().unwrap_or_default().into());
        }
        table["env"] = value(inline);
    }
    let servers = doc.entry("mcp_servers").or_insert(Item::Table(Table::new()));
    let Some(servers) = servers.as_table_mut() else {
        return Err("its `mcp_servers` is not a table. Add the entry by hand".into());
    };
    servers.set_implicit(true);
    let action = match servers.get(NAME) {
        _ if existing.is_none_or(|t| t.trim().is_empty()) => Action::Create,
        Some(old) if same(old, entry) => Action::Unchanged,
        Some(_) => Action::Update,
        None => Action::Merge,
    };
    if action != Action::Unchanged {
        servers.insert(NAME, Item::Table(table));
    }
    Ok((doc.to_string(), action))
}

/// Whether a TOML server entry says what `entry` says, however it is laid out.
fn same(old: &toml_edit::Item, entry: &Value) -> bool {
    let strings = |item: Option<&toml_edit::Item>| -> Vec<String> {
        item.and_then(|i| i.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default()
    };
    let want_args: Vec<String> =
        entry["args"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(String::from)).collect();
    let env_of = |item: Option<&toml_edit::Item>| -> Vec<(String, String)> {
        item.and_then(|i| i.as_table_like())
            .map(|t| t.iter().map(|(k, v)| (k.to_string(), v.as_str().unwrap_or_default().to_string())).collect())
            .unwrap_or_default()
    };
    let want_env: Vec<(String, String)> = entry
        .get("env")
        .and_then(Value::as_object)
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string())).collect())
        .unwrap_or_default();
    old.get("command").and_then(|c| c.as_str()) == entry["command"].as_str()
        && strings(old.get("args")) == want_args
        && env_of(old.get("env")) == want_env
}

#[cfg(test)]
#[path = "merge_tests.rs"]
mod tests;
