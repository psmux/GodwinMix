//! Turning a method's params schema into a tool's input schema.
//!
//! Two passes. `inline_refs` replaces every `$ref` with what it points at,
//! because most MCP clients cannot follow one. `prune` then cuts what is
//! nested too deep to be worth its bytes down to its type and description,
//! because the hot list is charged for on every call.
//!
//! Both passes only ever produce valid schemas. An earlier version cut by
//! counting every JSON level and replaced whatever value it stopped at, which
//! once turned the word `"string"` inside a `type` into an object. Claude Code
//! drops a tool whose schema does not validate, and that hid `take`.

use serde_json::{json, Map, Value};

/// How many references inlining follows down one path before it gives up. A
/// recursive type would otherwise expand for ever; none of ours is, and a
/// guard is cheaper than finding out the hard way in a client's context
/// window.
const MAX_REFS: usize = 8;

/// How many schema levels below the arguments object are shown in full. A
/// field of a field of an argument still shows its own fields; below that a
/// schema keeps its type and its description, which is what a model reads.
pub const MAX_SCHEMA_DEPTH: usize = 4;

/// Replace every `$ref: "#/$defs/X"` with the definition itself.
///
/// `depth` is the number of references already followed.
pub fn inline_refs(schema: &Value, defs: &Map<String, Value>, depth: usize) -> Value {
    match schema {
        Value::Object(map) => {
            if let Some(name) = map.get("$ref").and_then(Value::as_str).and_then(def_name) {
                if depth >= MAX_REFS {
                    return json!({ "type": "object" });
                }
                if let Some(target) = defs.get(name) {
                    let mut inlined = inline_refs(target, defs, depth + 1);
                    // A sibling `description` on the reference is the field's
                    // own documentation and beats the type's.
                    if let (Some(out), Some(doc)) = (inlined.as_object_mut(), map.get("description")) {
                        out.insert("description".into(), doc.clone());
                    }
                    return inlined;
                }
            }
            let out = map
                .iter()
                .filter(|(key, _)| key.as_str() != "$defs")
                .map(|(key, value)| (key.clone(), inline_refs(value, defs, depth)))
                .collect();
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(|v| inline_refs(v, defs, depth)).collect()),
        other => other.clone(),
    }
}

fn def_name(reference: &str) -> Option<&str> {
    reference.strip_prefix("#/$defs/")
}

/// Keys whose value is one schema.
const ONE: [&str; 3] = ["items", "additionalProperties", "not"];
/// Keys whose value is a list of schemas.
const MANY: [&str; 4] = ["anyOf", "oneOf", "allOf", "prefixItems"];

/// Cut every schema deeper than `MAX_SCHEMA_DEPTH` down to its type and
/// description. `depth` is 0 for the arguments object itself.
pub fn prune(schema: &Value, depth: usize) -> Value {
    // `true` and `false` are valid JSON Schema, but the MCP TypeScript SDK
    // that opencode is built on wants every property to be an object, and
    // refused the whole tool list over one `"exit": true`.
    let map = match schema {
        Value::Object(map) => map,
        Value::Bool(true) => return json!({}),
        Value::Bool(false) => return json!({ "not": {} }),
        other => return other.clone(),
    };
    if depth > MAX_SCHEMA_DEPTH {
        return summary(map);
    }
    let mut out = map.clone();
    if let Some(Value::Object(props)) = out.get_mut("properties") {
        for value in props.values_mut() {
            *value = prune(value, depth + 1);
        }
    }
    for key in ONE {
        if let Some(value) = out.get_mut(key) {
            *value = prune(value, depth + 1);
        }
    }
    for key in MANY {
        if let Some(Value::Array(list)) = out.get_mut(key) {
            for value in list.iter_mut() {
                *value = prune(value, depth + 1);
            }
        }
    }
    Value::Object(out)
}

/// A schema reduced to what a model reads: its type, when it has a plain one,
/// and its description. Both are valid on their own; `{}` takes anything.
fn summary(map: &Map<String, Value>) -> Value {
    let mut out = Map::new();
    match map.get("type") {
        Some(t @ Value::String(_)) => {
            out.insert("type".into(), t.clone());
        }
        Some(Value::Array(types)) if types.iter().all(Value::is_string) => {
            out.insert("type".into(), Value::Array(types.clone()));
        }
        _ => {}
    }
    if let Some(doc) = map.get("description") {
        out.insert("description".into(), doc.clone());
    }
    Value::Object(out)
}

#[cfg(test)]
#[path = "mcp_schema_tests.rs"]
mod tests;
