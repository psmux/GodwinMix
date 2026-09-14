//! The MCP tool list, generated from the method table.
//!
//! There is no static `Vec` of tools anywhere. A tool exists because a method
//! carries an `McpBinding`, its input schema is the method's params schema
//! with the `$defs` inlined, and its annotations are read off the same flags
//! the server enforces, so a `destructiveHint` cannot be a lie.
//!
//! Two profiles, because 09 section 5 item 1 puts a number on it: `standard`
//! is at most twelve hot tools, `minimal` is five for a 4,096 token context,
//! and everything else is behind `search_tools`. The hot list is a pure
//! function of the profile, so adding a source or a plugin never changes it
//! and the client's prompt cache stays valid.

use crate::method::{Registry, Tier};
use crate::scope::Profile;
use schemars::generate::SchemaSettings;
use serde_json::{json, Map, Value};

/// The rough token budget, as bytes. Four bytes to a token is the proxy the
/// plan uses, so 16,000 bytes is about 4,000 tokens and 4,800 is about 1,200.
pub const STANDARD_BYTES: usize = 16_000;
pub const MINIMAL_BYTES: usize = 4_800;

/// The most hot tools either profile may carry, `search_tools` included.
pub const STANDARD_TOOLS: usize = 12;
pub const MINIMAL_TOOLS: usize = 5;

/// The one tool that is not a method: it searches the rest.
pub const SEARCH_TOOL: &str = "search_tools";

fn search_tool(hot: usize, total: usize) -> Value {
    json!({
        "name": SEARCH_TOOL,
        "description": format!(
            "Find a tool that is not in this list. {hot} of the mixer's {total} tools are \
             shown; the rest (outputs, media, ad breaks, seeking, snapshots) are reachable \
             by searching here and then calling the name that comes back. Search by what \
             you want to do, in plain words: \"stop sending to youtube\", \"play a clip\". \
             Each match carries its description and input schema, ready to call."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "What you are trying to do, in plain words or as a \
                                    method name such as output.remove."
                },
                "limit": {
                    "type": "integer",
                    "description": "How many matches to return. Default 5, at most 20."
                }
            },
            "required": ["query"],
            "additionalProperties": false
        },
        "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true }
    })
}

/// Every tool, hot or not, as `search_tools` and `tools/call` see them.
pub fn all_tools<C>(registry: &Registry<C>) -> Vec<Value> {
    let mut g = SchemaSettings::draft2020_12().into_generator();
    let mut built: Vec<(String, Value)> = Vec::new();
    for m in registry.iter() {
        let Some(binding) = &m.mcp else { continue };
        let schema = (m.params)(&mut g);
        built.push((
            binding.tool.to_string(),
            json!({
                "name": binding.tool,
                "description": binding.description,
                "method": m.name,
                "inputSchema": schema,
                "annotations": {
                    "readOnlyHint": !m.mutating,
                    "destructiveHint": m.destructive,
                    "idempotentHint": m.idempotent,
                    "openWorldHint": false,
                },
            }),
        ));
    }
    let defs = g.take_definitions(true);
    built
        .into_iter()
        .map(|(_, mut tool)| {
            if let Some(schema) = tool.get_mut("inputSchema") {
                *schema = inline_refs(schema, &defs, 0);
            }
            tool
        })
        .collect()
}

/// The hot list for a profile: what `tools/list` answers with.
///
/// Ordered by profile then by method name, so two runs of the same build give
/// byte identical output and a client's prompt cache survives a reconnect.
pub fn tools<C>(registry: &Registry<C>, profile: Profile) -> Vec<Value> {
    let wanted = |tier: Tier| {
        matches!(
            (profile, tier),
            (_, Tier::Minimal) | (Profile::Standard, Tier::Standard)
        )
    };
    let hot_names: Vec<&str> = registry
        .iter()
        .filter_map(|m| m.mcp.as_ref())
        .filter(|b| wanted(b.tier))
        .map(|b| b.tool)
        .collect();
    let all = all_tools(registry);
    let total = all.len() + 1;
    let mut hot: Vec<Value> = all
        .into_iter()
        .filter(|t| hot_names.contains(&t["name"].as_str().unwrap_or_default()))
        .map(strip_method)
        .collect();
    hot.push(search_tool(hot.len() + 1, total));
    hot
}

/// `method` is how the server routes a call; an agent has no use for it and
/// every byte in the hot list is charged for.
fn strip_method(mut tool: Value) -> Value {
    if let Some(map) = tool.as_object_mut() {
        map.remove("method");
    }
    tool
}

/// The method behind a tool name.
pub fn method_for<C>(registry: &Registry<C>, tool: &str) -> Option<&'static str> {
    registry
        .iter()
        .find(|m| m.mcp.as_ref().is_some_and(|b| b.tool == tool))
        .map(|m| m.name)
}

/// What `search_tools` answers with: the tools whose name, description or
/// method mention the query, best first.
pub fn search<C>(registry: &Registry<C>, query: &str, limit: usize) -> Vec<Value> {
    let words: Vec<String> = query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(String::from)
        .collect();
    let mut scored: Vec<(usize, Value)> = all_tools(registry)
        .into_iter()
        .map(|tool| (score(&tool, &words), tool))
        .filter(|(score, _)| *score > 0)
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| name_of(&a.1).cmp(name_of(&b.1))));
    scored
        .into_iter()
        .take(limit.clamp(1, 20))
        .map(|(_, t)| t)
        .collect()
}

fn name_of(tool: &Value) -> &str {
    tool["name"].as_str().unwrap_or_default()
}

fn score(tool: &Value, words: &[String]) -> usize {
    let name = name_of(tool).to_lowercase();
    let method = tool["method"].as_str().unwrap_or_default().to_lowercase();
    let description = tool["description"]
        .as_str()
        .unwrap_or_default()
        .to_lowercase();
    let mut score = 0;
    for word in words {
        if name.contains(word.as_str()) || method.contains(word.as_str()) {
            score += 4;
        } else if description.contains(word.as_str()) {
            score += 1;
        }
    }
    score
}

/// How deep a schema may nest before inlining gives up. A recursive type would
/// otherwise expand for ever; none of ours is, and a guard is cheaper than
/// finding out the hard way in a client's context window.
const MAX_DEPTH: usize = 8;

/// Replace every `$ref: "#/$defs/X"` with the definition itself.
///
/// MCP clients want a self contained input schema: most do not resolve `$ref`
/// at all, and a tool whose schema is one `$ref` reads to a model as a tool
/// that takes anything.
pub fn inline_refs(schema: &Value, defs: &Map<String, Value>, depth: usize) -> Value {
    if depth > MAX_DEPTH {
        return json!({ "type": "object" });
    }
    match schema {
        Value::Object(map) => {
            if let Some(name) = map.get("$ref").and_then(Value::as_str).and_then(def_name) {
                if let Some(target) = defs.get(name) {
                    let mut inlined = inline_refs(target, defs, depth + 1);
                    // A sibling `description` on the reference is the field's
                    // own documentation and beats the type's.
                    if let (Some(out), Some(doc)) =
                        (inlined.as_object_mut(), map.get("description"))
                    {
                        out.insert("description".into(), doc.clone());
                    }
                    return inlined;
                }
            }
            let mut out = Map::new();
            for (key, value) in map {
                if key == "$defs" {
                    continue;
                }
                out.insert(key.clone(), inline_refs(value, defs, depth + 1));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| inline_refs(v, defs, depth + 1))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn def_name(reference: &str) -> Option<&str> {
    reference.strip_prefix("#/$defs/")
}

/// The bytes a tool list costs, which is what the budget test measures.
pub fn wire_size(tools: &[Value]) -> usize {
    serde_json::to_string(&json!({ "tools": tools }))
        .map(|s| s.len())
        .unwrap_or(0)
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
        assert!(
            out.get("$defs").is_none(),
            "$defs must not survive inlining"
        );
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
}
