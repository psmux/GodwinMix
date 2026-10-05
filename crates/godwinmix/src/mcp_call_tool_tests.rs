//! Tests for `mcp_call_tool.rs`.
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

/// `get_scene {"scene": "Live"}` reaches the scene: its schema says
/// `scene`, and the path used to be filled from `id` only.
#[test]
fn a_scene_tool_fills_its_path_from_scene() {
    use godwinmix_protocol::scope::Profile;
    let s = super::super::Server::new("http://127.0.0.1:1", None, Profile::Standard);
    let p = s.plan("get_scene", &json!({"scene": "Live"})).unwrap();
    assert_eq!(p.path, "/api/v1/scenes/Live");
    assert!(p.args.get("scene").is_none(), "the scene is in the path, and twice is a duplicate field");
    // The same for a name in the path: `remove_media` was refused with a
    // duplicate `name` when Opus cleaned up after itself.
    let p = s.plan("remove_media", &json!({"name": "bgtest-a.svg"})).unwrap();
    assert!(p.path.contains("bgtest-a.svg"), "{}", p.path);
    assert!(p.args.get("name").is_none(), "{:?}", p.args);
}

#[test]
fn a_refusal_shows_the_shape_to_send() {
    for sent in [json!({}), json!("list_scenes"), json!({"name": "call_tool"}), json!({"name": "x", "arguments": [1]})] {
        let msg = unwrap(&sent).unwrap_err();
        assert!(msg.contains("\"arguments\""), "{msg}");
    }
}
