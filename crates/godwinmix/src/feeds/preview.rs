//! What `feed.test` shows of a document: its top keys, a cut down copy, and
//! the paths in it with an example each, which are the paths `select` takes.

use godwinmix_protocol::feeds::PathExample;
use serde_json::{Map, Value};

const LIST_SHOWN: usize = 5;
const STRING_SHOWN: usize = 200;
const DEPTH: usize = 6;
const PATHS: usize = 200;

pub fn keys(doc: &Value) -> Vec<String> {
    match doc {
        Value::Object(map) => map.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

/// The document with lists cut to their first few and long strings shortened.
pub fn cut(v: &Value) -> Value {
    cut_at(v, 0)
}

fn cut_at(v: &Value, depth: usize) -> Value {
    match v {
        Value::String(s) if s.chars().count() > STRING_SHOWN => {
            Value::String(format!("{}...", s.chars().take(STRING_SHOWN).collect::<String>()))
        }
        _ if depth >= DEPTH && (v.is_array() || v.is_object()) => Value::String(super::path::kind(v)),
        Value::Array(items) => {
            let mut out: Vec<Value> = items.iter().take(LIST_SHOWN).map(|i| cut_at(i, depth + 1)).collect();
            if items.len() > LIST_SHOWN {
                out.push(Value::String(format!("(and {} more)", items.len() - LIST_SHOWN)));
            }
            Value::Array(out)
        }
        Value::Object(map) => {
            Value::Object(map.iter().map(|(k, v)| (k.clone(), cut_at(v, depth + 1))).collect::<Map<_, _>>())
        }
        other => other.clone(),
    }
}

/// Every path to a value, a list going in by its first element as `[]`.
pub fn paths(doc: &Value) -> Vec<PathExample> {
    let mut out = Vec::new();
    walk(doc, String::new(), 0, &mut out);
    out
}

fn walk(v: &Value, here: String, depth: usize, out: &mut Vec<PathExample>) {
    if out.len() >= PATHS || depth > DEPTH {
        return;
    }
    match v {
        Value::Object(map) => {
            for (k, child) in map {
                walk(child, join(&here, k), depth + 1, out);
            }
        }
        Value::Array(items) => {
            let scalars = items.iter().all(|i| !i.is_object() && !i.is_array());
            if scalars || items.is_empty() {
                out.push(PathExample { path: here, example: cut(v) });
            } else {
                out.push(PathExample { path: here.clone(), example: Value::String(super::path::kind(v)) });
                walk(&items[0], format!("{here}[]"), depth + 1, out);
            }
        }
        scalar => out.push(PathExample { path: here, example: cut(scalar) }),
    }
}

fn join(here: &str, key: &str) -> String {
    let plain = !key.is_empty() && key.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-');
    match (here.is_empty(), plain) {
        (true, true) => key.to_string(),
        (false, true) => format!("{here}.{key}"),
        (_, false) => format!("{here}[\"{key}\"]"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn paths_go_into_lists_by_their_first_element() {
        let doc = json!({ "title": "N", "items": [ { "title": "a" }, { "title": "b" } ], "tags": ["x"], "Home team": 1 });
        let got: Vec<String> = paths(&doc).into_iter().map(|p| p.path).collect();
        assert_eq!(got, vec!["[\"Home team\"]", "items", "items[].title", "tags", "title"]);
    }

    #[test]
    fn a_long_list_is_cut_and_says_how_many_more() {
        let doc = json!({ "n": (0..9).collect::<Vec<_>>() });
        assert_eq!(cut(&doc)["n"].as_array().unwrap().last().unwrap(), "(and 4 more)");
    }
}
