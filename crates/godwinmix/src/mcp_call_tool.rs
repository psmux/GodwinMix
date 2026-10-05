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
    fn a_refusal_shows_the_shape_to_send() {
        for sent in [json!({}), json!("list_scenes"), json!({"name": "call_tool"}), json!({"name": "x", "arguments": [1]})] {
            let msg = unwrap(&sent).unwrap_err();
            assert!(msg.contains("\"arguments\""), "{msg}");
        }
    }
}
