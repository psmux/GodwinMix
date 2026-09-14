//! `openapi.json` for the REST layer, from the same method table.
//!
//! Not a second description of the API: every path, every body and every
//! schema here comes out of `Registry`, so Swagger UI, a client generator and
//! `protocol.json` cannot disagree. 05 section 2 asks for this artefact beside
//! `protocol.json`, checked into the repository and verified by CI.
//!
//! OpenAPI 3.1, because it is the first version whose schema dialect is JSON
//! Schema proper, which is what `schemars` produces. `$defs` become
//! `components/schemas` and every `$ref` is rewritten to match.

use crate::error::ErrorCode;
use crate::method::Registry;
use crate::{API_COMPATIBLE, API_LEVEL};
use schemars::generate::SchemaSettings;
use serde_json::{json, Map, Value};

/// Build the document.
pub fn openapi<C>(registry: &Registry<C>) -> Value {
    let mut g = SchemaSettings::draft2020_12().into_generator();
    let mut paths: Map<String, Value> = Map::new();

    for m in registry.iter() {
        let Some(rest) = &m.rest else { continue };
        let params = (m.params)(&mut g);
        let result = (m.result)(&mut g);
        let entry = paths.entry(rest.path.clone()).or_insert_with(|| json!({}));
        let Some(item) = entry.as_object_mut() else {
            continue;
        };
        item.insert(
            rest.http.to_lowercase(),
            operation(m, rest, &params, &result),
        );
    }

    let mut schemas = Value::Object(g.take_definitions(true));
    schemas = rewrite_refs(&schemas);
    let paths = rewrite_refs(&Value::Object(paths));

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "GodwinMix control API",
            "version": format!("{API_LEVEL}"),
            "summary": "The REST layer of the GodwinMix control protocol.",
            "description": format!(
                "Generated from the method table in crates/godwinmix-protocol/. api_level {API_LEVEL}, \
                 compatible from {API_COMPATIBLE}. The same methods are reachable over \
                 JSON-RPC on the /rpc WebSocket; see protocol.md. The paths here come from \
                 the noun.verb transform rule, so `source.list` is `GET /api/v1/sources` \
                 and `source.audio.set` is `POST /api/v1/sources/{{id}}/audio`.\n\n\
                 Four keys are accepted in the body of every call: `trace_id`, \
                 `idempotency_key` on anything mutating, `dry_run` on anything destructive, \
                 and `confirm` where the token's policy requires it."
            ),
            "license": { "name": "Apache-2.0" }
        },
        "servers": [{ "url": "/", "description": "the mixer's control port" }],
        "security": [{ "bearer": [] }],
        "components": {
            "securitySchemes": {
                "bearer": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "The control token. A GET may carry it as ?token= \
                                    instead, because a browser opening a WebSocket or an \
                                    <img> tag cannot set a header."
                }
            },
            "schemas": schemas,
            "responses": { "Error": error_response() }
        },
        "paths": paths,
    })
}

fn operation<C>(
    m: &crate::method::MethodDef<C>,
    rest: &crate::method::Rest,
    params: &Value,
    result: &Value,
) -> Value {
    let noun = m.name.split('.').next().unwrap_or(m.name);
    let mut op = Map::new();
    op.insert("operationId".into(), json!(m.name));
    op.insert("summary".into(), json!(m.summary));
    op.insert("tags".into(), json!([noun]));
    op.insert("description".into(), json!(description(m)));
    op.insert("x-scope".into(), json!(m.scope.as_str()));
    op.insert("x-destructive".into(), json!(m.destructive));
    op.insert("x-idempotent".into(), json!(m.idempotent));

    let mut parameters: Vec<Value> = Vec::new();
    if rest.path.contains("{id}") {
        parameters.push(json!({
            "name": "id",
            "in": "path",
            "required": true,
            "schema": { "type": "string" },
            "description": "The id, as listed by the matching list call."
        }));
    }
    if rest.http == "GET" {
        parameters.extend(query_parameters(params));
    } else {
        op.insert(
            "requestBody".into(),
            json!({ "required": false, "content": { "application/json": { "schema": params } } }),
        );
    }
    if !parameters.is_empty() {
        op.insert("parameters".into(), json!(parameters));
    }
    op.insert(
        "responses".into(),
        json!({
            "200": {
                "description": "the full resulting object, so no follow up read is needed",
                "content": { "application/json": { "schema": result } }
            },
            "default": { "$ref": "#/components/responses/Error" }
        }),
    );
    Value::Object(op)
}

fn description<C>(m: &crate::method::MethodDef<C>) -> String {
    let mut text = format!(
        "JSON-RPC method `{}`. Needs the `{}` scope.",
        m.name,
        m.scope.as_str()
    );
    if m.destructive {
        text.push_str(
            " Destructive: accepts `dry_run: true`, and on a token whose policy is \
             `confirm = required` it is refused once with -32020 and a confirm token.",
        );
    }
    if m.mutating {
        text.push_str(" Accepts `idempotency_key`, honoured for 24 hours.");
    }
    text
}

/// A GET carries its params in the query, one parameter per top level property
/// of the params schema.
///
/// Only a `$ref` into `$defs` or an inline object is understood, which is what
/// every method in the table produces. Anything else contributes nothing
/// rather than producing a parameter nobody can send.
fn query_parameters(params: &Value) -> Vec<Value> {
    let Some(properties) = params.get("properties").and_then(Value::as_object) else {
        return Vec::new();
    };
    let required: Vec<&str> = params
        .get("required")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    properties
        .iter()
        .filter(|(name, _)| name.as_str() != "id")
        .map(|(name, schema)| {
            json!({
                "name": name,
                "in": "query",
                "required": required.contains(&name.as_str()),
                "schema": schema,
                "description": schema.get("description").and_then(Value::as_str).unwrap_or(""),
            })
        })
        .collect()
}

fn error_response() -> Value {
    let codes: Vec<Value> = ErrorCode::ALL
        .iter()
        .map(|c| {
            json!(format!(
                "{}: {} (HTTP {})",
                c.number(),
                c.meaning(),
                c.http_status()
            ))
        })
        .collect();
    json!({
        "description": "One error shape everywhere. The message names the current state and \
                        the next step, and an unknown id lists the ids that would have worked.",
        "content": {
            "application/json": {
                "schema": {
                    "type": "object",
                    "required": ["error"],
                    "properties": {
                        "error": {
                            "type": "object",
                            "required": ["code", "message", "data"],
                            "properties": {
                                "code": { "type": "integer", "description": "JSON-RPC code.", "examples": codes },
                                "message": { "type": "string" },
                                "data": {
                                    "type": "object",
                                    "description": "Always carries `retryable`; carries `valid` \
                                                    on an unknown id and `confirm_token` on -32020."
                                }
                            }
                        },
                        "trace_id": { "type": "string" }
                    }
                }
            }
        }
    })
}

/// `#/$defs/X` is where schemars puts a shared type; OpenAPI wants
/// `#/components/schemas/X`.
fn rewrite_refs(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, v) in map {
                if key == "$ref" {
                    if let Some(name) = v.as_str().and_then(|r| r.strip_prefix("#/$defs/")) {
                        out.insert(
                            key.clone(),
                            Value::String(format!("#/components/schemas/{name}")),
                        );
                        continue;
                    }
                }
                out.insert(key.clone(), rewrite_refs(v));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(rewrite_refs).collect()),
        other => other.clone(),
    }
}

/// The bytes written to `openapi.json`, ending in a newline.
pub fn json_text(doc: &Value) -> String {
    let mut text = serde_json::to_string_pretty(doc).unwrap_or_default();
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ref_is_rewritten_into_the_components_section() {
        let before = json!({
            "properties": { "x": { "$ref": "#/$defs/SourceStatus" } },
            "list": [{ "$ref": "#/$defs/Ext" }]
        });
        let after = rewrite_refs(&before);
        assert_eq!(
            after["properties"]["x"]["$ref"],
            "#/components/schemas/SourceStatus"
        );
        assert_eq!(after["list"][0]["$ref"], "#/components/schemas/Ext");
        // A reference that is already an OpenAPI one is left alone.
        let already = json!({ "$ref": "#/components/responses/Error" });
        assert_eq!(
            rewrite_refs(&already)["$ref"],
            "#/components/responses/Error"
        );
    }

    #[test]
    fn a_get_carries_its_params_in_the_query_and_never_duplicates_the_path_id() {
        let params = json!({
            "type": "object",
            "required": ["id"],
            "properties": {
                "id": { "type": "string" },
                "width": { "type": "integer", "description": "pixels across" }
            }
        });
        let query = query_parameters(&params);
        assert_eq!(
            query.len(),
            1,
            "the path id must not be a query parameter too"
        );
        assert_eq!(query[0]["name"], "width");
        assert_eq!(query[0]["in"], "query");
        assert_eq!(query[0]["required"], false);
        assert_eq!(query[0]["description"], "pixels across");
        // A schema with no properties at all contributes nothing.
        assert!(query_parameters(&json!({ "type": "object" })).is_empty());
    }
}
