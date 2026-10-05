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
    let Value::Object(map) = schema else { return schema.clone() };
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
mod tests {
    use super::*;

    #[test]
    fn a_ref_is_replaced_by_what_it_points_at() {
        let defs: Map<String, Value> = serde_json::from_value(json!({
            "Inner": { "type": "string", "enum": ["a", "b"], "description": "the type's doc" }
        }))
        .unwrap();
        let schema = json!({
            "type": "object",
            "properties": { "x": { "$ref": "#/$defs/Inner", "description": "the field's doc" } }
        });
        let out = inline_refs(&schema, &defs, 0);
        assert_eq!(out["properties"]["x"]["type"], "string");
        assert_eq!(out["properties"]["x"]["enum"][1], "b");
        // The field's own documentation wins, because that is the one written
        // about this use of the type.
        assert_eq!(out["properties"]["x"]["description"], "the field's doc");
        assert!(out.get("$defs").is_none(), "$defs must not survive inlining");
    }

    #[test]
    fn a_ref_that_points_nowhere_is_left_alone_rather_than_dropped() {
        let out = inline_refs(&json!({ "$ref": "#/$defs/Missing" }), &Map::new(), 0);
        assert_eq!(out["$ref"], "#/$defs/Missing");
    }

    #[test]
    fn inlining_gives_up_rather_than_recursing_for_ever() {
        let defs: Map<String, Value> =
            serde_json::from_value(json!({ "Loop": { "$ref": "#/$defs/Loop" } })).unwrap();
        let out = inline_refs(&json!({ "$ref": "#/$defs/Loop" }), &defs, 0);
        assert_eq!(out["type"], "object");
    }

    /// Deep nesting with no reference in it comes through inlining whole.
    #[test]
    fn deep_nesting_without_a_reference_is_left_whole() {
        let mut schema = json!({ "type": "string" });
        for _ in 0..12 {
            schema = json!({ "anyOf": [schema, { "type": "null" }] });
        }
        assert_eq!(inline_refs(&schema, &Map::new(), 0), schema);
    }

    /// Pruning keeps a deep schema's type and words and drops its insides,
    /// and never puts anything but a type name under `type`.
    #[test]
    fn pruning_keeps_the_type_and_the_description() {
        let mut schema = json!({ "type": ["string", "null"], "description": "deep", "enum": ["a"] });
        for _ in 0..8 {
            schema = json!({ "type": "object", "properties": { "x": schema } });
        }
        let mut out = prune(&schema, 0);
        for _ in 0..=MAX_SCHEMA_DEPTH {
            out = out["properties"]["x"].clone();
        }
        assert_eq!(out, json!({ "type": "object" }), "below the limit only the type is left");
        // A property literally named `type` is a schema, not a type name.
        let named = prune(&json!({ "type": "object", "properties": { "type": { "type": "string" } } }), 0);
        assert_eq!(named["properties"]["type"]["type"], "string");
    }
}
