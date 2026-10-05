//! Every tool's input schema is one a strict client accepts.
//!
//! Claude Code checks each schema it is given and drops a tool whose schema
//! does not validate, without a word to the model beyond "not available".
//! That happened to `take`: a `type` deep inside its `transition` had become
//! an object. These tests walk every schema, hot or searchable, the way a
//! validator would.

use super::*;
use godwinmix_protocol::mcp_tools;

/// The places inside a schema that hold further schemas.
fn children(schema: &Map<String, Value>) -> Vec<(String, &Value)> {
    let mut out = Vec::new();
    if let Some(Value::Object(props)) = schema.get("properties") {
        out.extend(props.iter().map(|(k, v)| (format!("properties.{k}"), v)));
    }
    for key in ["items", "additionalProperties", "not"] {
        if let Some(v @ Value::Object(_)) = schema.get(key) {
            out.push((key.to_string(), v));
        }
    }
    for key in ["anyOf", "oneOf", "allOf", "prefixItems"] {
        if let Some(Value::Array(list)) = schema.get(key) {
            out.extend(list.iter().enumerate().map(|(i, v)| (format!("{key}[{i}]"), v)));
        }
    }
    out
}

/// Every problem a validator would raise, as `path: what`.
fn problems(path: &str, schema: &Value, found: &mut Vec<String>) {
    // `true` is the schema that takes anything, which every validator knows.
    if schema.is_boolean() {
        return;
    }
    let Some(map) = schema.as_object() else {
        found.push(format!("{path}: a schema has to be an object, not {schema}"));
        return;
    };
    match map.get("type") {
        None | Some(Value::String(_)) => {}
        Some(Value::Array(types)) if types.iter().all(Value::is_string) => {}
        Some(other) => found.push(format!("{path}.type is {other}, which is not a type name")),
    }
    if map.contains_key("$ref") {
        found.push(format!("{path}: an unresolved $ref, which most clients cannot follow"));
    }
    for (key, child) in children(map) {
        problems(&format!("{path}.{key}"), child, found);
    }
}

#[test]
fn every_tool_schema_is_one_a_strict_client_accepts() {
    let server = Server::new("http://127.0.0.1:1", None, Profile::Standard);
    let mut found = Vec::new();
    for tool in mcp_tools::all_tools(&server.registry) {
        let name = tool["name"].as_str().unwrap_or("?").to_string();
        let schema = &tool["inputSchema"];
        if schema["type"] != "object" {
            found.push(format!("{name}: the input schema's top level is not an object"));
        }
        for key in ["anyOf", "oneOf", "allOf"] {
            if schema.get(key).is_some() {
                found.push(format!("{name}: {key} at the top level, which the Anthropic API refuses"));
            }
        }
        problems(&name, schema, &mut found);
    }
    assert!(found.is_empty(), "schemas a client would drop:\n{}", found.join("\n"));
}

/// The one that broke, by name: `take` with a transition by name or object.
#[test]
fn takes_transition_says_a_name_or_an_object() {
    let server = Server::new("http://127.0.0.1:1", None, Profile::Standard);
    let take = server.tools().into_iter().find(|t| t["name"] == "take").expect("take is hot");
    let text = take["inputSchema"]["properties"]["transition"].to_string();
    assert!(text.contains("\"string\""), "a transition by name is gone: {text}");
    assert!(text.contains("duration_ms"), "the object form lost its fields: {text}");
}
