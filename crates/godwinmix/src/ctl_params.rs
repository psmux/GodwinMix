//! `--param key=value` on `gmx ctl source add` and `gmx ctl source set`.
//!
//! A value that reads as JSON is taken as JSON, so `size=44`, `shadow=true`
//! and `items=["One","Two"]` arrive as a number, a boolean and a list.
//! Anything else is a string, so `text=Polls close at ten` needs no quotes
//! beyond the shell's own. A dotted name sets one key inside a table:
//! `fields.headline=Polls close` is `{"fields": {"headline": "Polls close"}}`,
//! which `source set` merges, so a graphic's other fields stay as they are.

use anyhow::Result;
use serde_json::{Map, Value};

/// The params a list of `key=value` flags names.
pub fn parse(flags: &[String]) -> Result<Map<String, Value>> {
    let mut out = Map::new();
    for flag in flags {
        let Some((key, value)) = flag.split_once('=') else {
            anyhow::bail!("--param {flag:?} has no `=`. Write it as key=value, for example --param size=44");
        };
        let key = key.trim();
        anyhow::ensure!(!key.is_empty(), "--param {flag:?} has no name before the `=`");
        let value = serde_json::from_str::<Value>(value).unwrap_or_else(|_| Value::String(value.replace("\\n", "\n")));
        insert(&mut out, key, value);
    }
    Ok(out)
}

/// Put `value` at the dotted path `key`, making tables on the way.
fn insert(map: &mut Map<String, Value>, key: &str, value: Value) {
    match key.split_once('.') {
        Some((head, rest)) if !head.is_empty() && !rest.is_empty() => {
            let slot = map.entry(head.to_string()).or_insert_with(|| Value::Object(Map::new()));
            if !slot.is_object() {
                *slot = Value::Object(Map::new());
            }
            if let Value::Object(inner) = slot {
                insert(inner, rest, value);
            }
        }
        _ => {
            map.insert(key.to_string(), value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_read_as_json_when_they_are_and_as_words_when_not() {
        let p = parse(&["size=44".into(), "shadow=true".into(), "items=[\"A\",\"B\"]".into(), "text=Ada\\nAnalyst".into()]).unwrap();
        assert_eq!(p["size"], 44);
        assert_eq!(p["shadow"], true);
        assert_eq!(p["items"][1], "B");
        assert_eq!(p["text"], "Ada\nAnalyst");
        assert!(parse(&["size".into()]).unwrap_err().to_string().contains("key=value"));
    }

    #[test]
    fn a_dotted_name_sets_one_key_inside_a_table() {
        let p = parse(&["fields.headline=Polls close".into(), "fields.score_home=2".into()]).unwrap();
        assert_eq!(p["fields"]["headline"], "Polls close");
        assert_eq!(p["fields"]["score_home"], 2);
    }
}
