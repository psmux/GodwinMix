//! How `source.set` lays new params over what a source has.
//!
//! The way a JSON merge patch does (RFC 7386): a key named is replaced, a
//! table named is merged key by key at every depth, and `null` removes a key.
//! So `{"fields": {"headline": "x"}}` changes one field of a graphic and
//! leaves its others alone, which is what a data feed setting one value by
//! its path needs, and `{"fields": {"headline": null}}` puts that one field
//! back to its default.

use godwinmix_protocol::error::RpcError;
use serde_json::Value;

/// Lay `value` over `params[key]`.
pub(super) fn merge(params: &mut toml::Table, key: &str, value: &Value, path: &str) -> Result<(), RpcError> {
    let here = if path.is_empty() { key.to_string() } else { format!("{path}.{key}") };
    match value {
        Value::Null => {
            params.remove(key);
        }
        Value::Object(map) if params.get(key).is_some_and(toml::Value::is_table) => {
            let Some(toml::Value::Table(table)) = params.get_mut(key) else { return Ok(()) };
            for (k, v) in map {
                merge(table, k, v, &here)?;
            }
        }
        other => {
            let v = toml::Value::try_from(without_nulls(other)).map_err(|e| {
                RpcError::invalid_params(format!("`params.{here}` is not something a config can hold: {e}"))
            })?;
            params.insert(key.to_string(), v);
        }
    }
    Ok(())
}

/// A value new to the params, with any `null` inside it dropped: there is
/// nothing under it to remove.
fn without_nulls(v: &Value) -> Value {
    match v {
        Value::Object(map) => Value::Object(map.iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.clone(), without_nulls(v))).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn table(t: &str) -> toml::Table {
        toml::from_str(t).unwrap()
    }

    #[test]
    fn one_field_changes_and_the_others_stay() {
        let mut p = table("[fields]\nhome = \"ARS\"\nscore_home = \"1\"");
        merge(&mut p, "fields", &json!({ "score_home": 2 }), "").unwrap();
        assert_eq!(p, table("[fields]\nhome = \"ARS\"\nscore_home = 2"));
    }

    #[test]
    fn null_removes_a_key_at_any_depth() {
        let mut p = table("size = 40\n[fields]\nhome = \"ARS\"\naway = \"CHE\"");
        merge(&mut p, "fields", &json!({ "home": null }), "").unwrap();
        merge(&mut p, "size", &Value::Null, "").unwrap();
        assert_eq!(p, table("[fields]\naway = \"CHE\""));
    }

    #[test]
    fn a_value_that_is_not_a_table_replaces_one() {
        let mut p = table("[fields]\nhome = \"ARS\"");
        merge(&mut p, "fields", &json!("flat"), "").unwrap();
        assert_eq!(p, table("fields = \"flat\""));
        merge(&mut p, "items", &json!(["a", "b"]), "").unwrap();
        assert_eq!(p.get("items").and_then(|v| v.as_array()).map(|a| a.len()), Some(2));
    }
}
