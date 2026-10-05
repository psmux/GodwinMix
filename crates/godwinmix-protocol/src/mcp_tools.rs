//! The MCP tool list, generated from the method table.
//!
//! There is no static `Vec` of tools anywhere. A tool exists because a method
//! carries an `McpBinding`, its input schema is the method's params schema
//! with the `$defs` inlined, and its annotations are read off the same flags
//! the server enforces, so a `destructiveHint` cannot be a lie.
//!
//! Three profiles, because 09 section 5 item 1 puts a number on it:
//! `standard` is the live mix with its graphics and scenes, `minimal` is six
//! tools for a 4,096 token context, `headend` is many shows at once. Everything
//! else is found with `search_tools` and run with `call_tool`. The hot list is
//! a pure function of the profile, so adding a source or a plugin never
//! changes it and the client's prompt cache stays valid.
//!
//! `call_tool` exists because most clients only let a model call what is in
//! its list. Claude Code and opencode both do: a tool `search_tools` found was
//! one the model could read about and never run, and Claude Opus said so and
//! stopped. Through `call_tool` every tool is one call away in any client.

use crate::method::{Registry, Tier};
use crate::scope::Profile;
use schemars::generate::SchemaSettings;
pub use crate::mcp_schema::inline_refs;
use crate::mcp_schema::prune;
use serde_json::{json, Value};

/// The rough token budget, as bytes. Four bytes to a token is the proxy the
/// plan uses, so 18,500 bytes is about 4,600 tokens and 6,000 is about 1,500.
/// Both grew when `call_tool` joined every profile and the standard list took
/// on the graphics and scene tools, because a tool behind a search was a tool
/// Claude Code and opencode could not run at all.
pub const STANDARD_BYTES: usize = 18_500;
pub const MINIMAL_BYTES: usize = 6_000;

/// The most hot tools a profile may carry, `call_tool` and `search_tools`
/// included. `headend` is held to the standard numbers.
pub const STANDARD_TOOLS: usize = 14;
pub const MINIMAL_TOOLS: usize = 6;

/// The two tools that are not methods: one searches the rest, one runs them.
pub const SEARCH_TOOL: &str = "search_tools";
pub const CALL_TOOL: &str = "call_tool";

fn call_tool() -> Value {
    json!({
        "name": CALL_TOOL,
        "description": "Run any GodwinMix tool by name, including one that is not in this \
            list: {\"name\": \"list_scenes\", \"arguments\": {}}. Use it for every tool \
            search_tools finds.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "The tool's name, such as get_scene." },
                "arguments": { "type": "object", "description": "The tool's own arguments." }
            },
            "required": ["name"]
        },
        "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": false }
    })
}

fn search_tool(hot: usize, total: usize) -> Value {
    json!({
        "name": SEARCH_TOOL,
        "description": format!(
            "Find a GodwinMix tool that is not in this list. {hot} of the mixer's {total}              tools are shown; the rest (outputs, the graphics gallery, media, ad breaks, feeds,              filters, layouts) are found here by what you want to do, in plain words: \"make a              lower third\", \"play a clip\". Each match carries its description and input              schema. Run one with call_tool."
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
            let one_show = addresses_one_show(tool["method"].as_str().unwrap_or_default());
            if let Some(schema) = tool.get_mut("inputSchema") {
                *schema = prune(&inline_refs(schema, &defs, 0), 0);
                // The request struct's own doc comment is written for whoever
                // maintains it; the tool's description is the agent's manual,
                // and every byte here is paid for on every call.
                if let Some(map) = schema.as_object_mut() {
                    map.remove("description");
                }
                if one_show {
                    add_show(schema);
                }
            }
            tool
        })
        .collect()
}

/// True for a method one show answers, as against the station in front of
/// the shows. Those tools take a `show` argument naming which show, and the
/// MCP server sends it as `?show=<id>`; the station's own methods (`show.*`,
/// `channel.*`, the governor) are about every show and take none.
pub fn addresses_one_show(method: &str) -> bool {
    !(method.starts_with("show.")
        || method.starts_with("channel.")
        || method.starts_with("governor."))
}

/// The `show` argument, added to an inlined input schema.
fn add_show(schema: &mut Value) {
    let Some(map) = schema.as_object_mut() else { return };
    let props = map.entry("properties").or_insert_with(|| json!({}));
    if let Some(props) = props.as_object_mut() {
        props.entry("show").or_insert_with(|| {
            json!({ "type": "string", "description": "Which show, from list_shows. Default: the first." })
        });
    }
}

/// The hot list for a profile: what `tools/list` answers with.
///
/// Ordered by profile then by method name, so two runs of the same build give
/// byte identical output and a client's prompt cache survives a reconnect.
pub fn tools<C>(registry: &Registry<C>, profile: Profile) -> Vec<Value> {
    let wanted = |tier: Tier| {
        matches!(
            (profile, tier),
            (_, Tier::Minimal) | (Profile::Standard, Tier::Standard) | (Profile::Headend, Tier::Headend)
        )
    };
    let hot_names: Vec<&str> = registry
        .iter()
        .filter_map(|m| m.mcp.as_ref())
        .filter(|b| wanted(b.tier))
        .map(|b| b.tool)
        .collect();
    let all = all_tools(registry);
    let total = all.len() + 2;
    let mut hot: Vec<Value> = all
        .into_iter()
        .filter(|t| hot_names.contains(&t["name"].as_str().unwrap_or_default()))
        .map(strip_method)
        .map(|tool| if profile == Profile::Minimal { without_show(tool) } else { tool })
        .collect();
    hot.push(call_tool());
    hot.push(search_tool(hot.len() + 1, total));
    hot
}

/// The minimal profile is for a small model working one show, and its
/// budget has no room for an argument that picks another. The tool still
/// takes it if sent; it is only not advertised.
fn without_show(mut tool: Value) -> Value {
    if let Some(props) = tool["inputSchema"]["properties"].as_object_mut() {
        props.remove("show");
    }
    tool
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
    scored.into_iter().take(limit.clamp(1, 20)).map(|(_, t)| t).collect()
}

fn name_of(tool: &Value) -> &str {
    tool["name"].as_str().unwrap_or_default()
}

fn score(tool: &Value, words: &[String]) -> usize {
    let name = name_of(tool).to_lowercase();
    let method = tool["method"].as_str().unwrap_or_default().to_lowercase();
    let description = tool["description"].as_str().unwrap_or_default().to_lowercase();
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

/// The bytes a tool list costs, which is what the budget test measures.
pub fn wire_size(tools: &[Value]) -> usize {
    serde_json::to_string(&json!({ "tools": tools })).map(|s| s.len()).unwrap_or(0)
}

