//! Tests for `mcp_schema.rs`.
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

/// A boolean schema becomes the object that means the same, because
/// opencode's MCP client refused a tool list with `"exit": true` in it.
#[test]
fn a_boolean_schema_becomes_an_object() {
    let out = prune(&json!({ "type": "object", "properties": { "exit": true, "never": false } }), 0);
    assert_eq!(out["properties"]["exit"], json!({}));
    assert_eq!(out["properties"]["never"], json!({ "not": {} }));
}
