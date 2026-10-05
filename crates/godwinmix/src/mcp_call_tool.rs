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
mod tests {
    use super::*;

    #[test]
    fn the_plain_form_is_name_and_arguments() {
        let (name, args) = unwrap(&json!({"name": "get_scene", "arguments": {"scene": "Live"}})).unwrap();
        assert_eq!((name.as_str(), args), ("get_scene", json!({"scene": "Live"})));
    }

    /// Every spelling a small model was seen to use, or is likely to.
    #[test]
    fn the_spellings_a_small_model_uses_mean_the_same() {
        let want = ("get_scene".to_string(), json!({"scene": "Live"}));
        for sent in [
            json!({"tool": "get_scene", "args": {"scene": "Live"}}),
            json!({"name": "mcp__godwinmix__get_scene", "arguments": {"scene": "Live"}}),
            json!({"name": "get_scene", "arguments": "{\"scene\": \"Live\"}"}),
            json!({"name": "get_scene", "scene": "Live"}),
            json!({"tool_name": " get_scene ", "params": {"scene": "Live"}}),
        ] {
            assert_eq!(unwrap(&sent).unwrap(), want, "{sent}");
        }
        assert_eq!(unwrap(&json!({"name": "agent_state"})).unwrap().1, json!({}));
        assert_eq!(unwrap(&json!({"name": "agent_state", "arguments": null})).unwrap().1, json!({}));
    }

    /// Through the server: `call_tool` reaches a tool that is not in the hot
    /// list, here `search_tools` itself, and a refusal is a tool result.
    #[tokio::test]
    async fn the_server_runs_the_named_tool() {
        use godwinmix_protocol::scope::Profile;
        let s = super::super::Server::new("http://127.0.0.1:1", None, Profile::Minimal);
        let r = s.call("call_tool", &json!({"name": "search_tools", "arguments": {"query": "apply a layout"}})).await;
        assert!(r.get("isError").is_none(), "{r:#?}");
        let text = r["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("apply_layout") && text.contains("call_tool"), "{text}");
        let r = s.call("call_tool", &json!({})).await;
        assert_eq!(r["isError"], true);
    }

    #[test]
    fn an_object_sent_as_text_is_an_object_again() {
        let sent = json!({
            "params": "{\"fields\": {\"name\": \"Ana\"}}",
            "exit": " {\"type\": \"fade\"} ",
            "items": "[\"a\", \"b\"]",
            "template": "{title} at {time}",
            "name": "lower third",
        });
        let out = unstring(&sent, None);
        assert_eq!(out["params"]["fields"]["name"], "Ana");
        assert_eq!(out["exit"]["type"], "fade");
        assert_eq!(out["items"], json!(["a", "b"]));
        assert_eq!(out["template"], "{title} at {time}", "a word that starts with a brace stays a word");
        assert_eq!(out["name"], "lower third");
        // Text the tool asks for as text is never parsed.
        let schema = json!({"properties": {"content": {"type": "string"}}});
        let obs = json!({"content": "{\"scenes\": []}"});
        assert_eq!(unstring(&obs, Some(&schema)), obs);
    }

    #[test]
    fn a_refusal_shows_the_shape_to_send() {
        for sent in [json!({}), json!("list_scenes"), json!({"name": "call_tool"}), json!({"name": "x", "arguments": [1]})] {
            let msg = unwrap(&sent).unwrap_err();
            assert!(msg.contains("\"arguments\""), "{msg}");
        }
    }
}
