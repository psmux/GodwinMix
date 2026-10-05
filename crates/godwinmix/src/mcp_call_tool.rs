//! `call_tool`: run any tool by name, for a client that only lets a model call
//! what is in its list.
//!
//! Forgiving on purpose. A small model writes `args` for `arguments`, sends
//! the arguments as a JSON string, or leaves them beside the name rather than
//! inside an object, and every one of those means the same thing. It also
//! copies the name its client shows it, `mcp__godwinmix__get_scene`, which is
//! the tool `get_scene`.

use serde_json::{json, Map, Value};

/// The keys a model uses for the tool's name, best first.
const NAME_KEYS: [&str; 3] = ["name", "tool", "tool_name"];
/// The keys a model uses for the tool's arguments, best first.
const ARG_KEYS: [&str; 5] = ["arguments", "args", "params", "parameters", "input"];
/// Prefixes clients put in front of a server's tool names.
const PREFIXES: [&str; 3] = ["mcp__godwinmix__", "godwinmix__", "godwinmix_"];

/// The tool to run and its arguments, or what to send instead.
pub fn unwrap(args: &Value) -> Result<(String, Value), String> {
    let Some(map) = args.as_object() else {
        return Err(usage("call_tool takes an object"));
    };
    let name = NAME_KEYS
        .iter()
        .find_map(|k| map.get(*k).and_then(Value::as_str))
        .map(clean_name)
        .filter(|n| !n.is_empty())
        .ok_or_else(|| usage("call_tool needs `name`, the tool to run"))?;
    if name == godwinmix_protocol::mcp_tools::CALL_TOOL {
        return Err(usage("call_tool runs another tool, not itself"));
    }
    Ok((name, arguments(map)?))
}

fn clean_name(raw: &str) -> String {
    let name = raw.trim();
    PREFIXES
        .iter()
        .find_map(|p| name.strip_prefix(p))
        .unwrap_or(name)
        .to_string()
}

fn arguments(map: &Map<String, Value>) -> Result<Value, String> {
    match ARG_KEYS.iter().find_map(|k| map.get(*k)) {
        Some(Value::Object(inner)) => Ok(Value::Object(inner.clone())),
        Some(Value::Null) => Ok(json!({})),
        Some(Value::String(text)) if text.trim().is_empty() => Ok(json!({})),
        Some(Value::String(text)) => match serde_json::from_str::<Value>(text) {
            Ok(v @ Value::Object(_)) => Ok(v),
            _ => Err(usage("`arguments` is a JSON object, not a string")),
        },
        Some(other) => Err(usage(&format!("`arguments` is a JSON object, not {other}"))),
        // Left beside the name: everything that is not the name.
        None => Ok(Value::Object(
            map.iter()
                .filter(|(k, _)| !NAME_KEYS.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        )),
    }
}

/// Arguments with an object or a list sent as JSON text put back as what
/// they are. Claude Code does this to an argument whose schema says it takes
/// anything: `"params": "{\"fields\": ...}"` and `"exit": "{\"type\": \"fade\"}"`
/// were both refused, and Opus had to retry each through `call_tool`. Only
/// text that parses as an object or a list is touched, so a word that merely
/// starts with a brace stays a word, and so does any argument the tool's
/// schema says is text, such as an OBS collection's `content`.
pub fn unstring(args: &Value, schema: Option<&Value>) -> Value {
    let Some(map) = args.as_object() else { return args.clone() };
    let is_text = |key: &str| {
        let t = schema.map(|s| &s["properties"][key]["type"]);
        t.is_some_and(|t| t == "string" || t.as_array().is_some_and(|a| a.iter().all(|x| x == "string" || x == "null")))
    };
    let fixed = map
        .iter()
        .map(|(k, v)| {
            let parsed = v
                .as_str()
                .map(str::trim)
                .filter(|t| !is_text(k) && (t.starts_with('{') || t.starts_with('[')));
            let value = match parsed.and_then(|t| serde_json::from_str::<Value>(t).ok()) {
                Some(inner @ (Value::Object(_) | Value::Array(_))) => inner,
                _ => v.clone(),
            };
            (k.clone(), value)
        })
        .collect();
    Value::Object(fixed)
}

fn usage(what: &str) -> String {
    format!(
        "{what}. Send {{\"name\": \"<tool>\", \"arguments\": {{...}}}}, for example \
         {{\"name\": \"list_scenes\", \"arguments\": {{}}}}. search_tools finds a tool's name."
    )
}

#[cfg(test)]
#[path = "mcp_call_tool_tests.rs"]
mod tests;
