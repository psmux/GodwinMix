//! Tests for `merge.rs`.
use super::*;
use serde_json::json;

fn entry() -> Value {
    json!({ "command": "/opt/godwinmix/godwinmix", "args": ["mcp"] })
}

#[test]
fn a_new_json_file_is_the_seed_and_the_entry() {
    let (text, action) = json(None, "mcpServers", &entry(), json!({})).unwrap();
    assert_eq!(action, Action::Create);
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["mcpServers"]["godwinmix"], entry());
}

/// Everything else in the file survives, and a second run changes nothing.
#[test]
fn json_keeps_every_other_key_and_is_idempotent() {
    let before = r#"{"theme": "dark", "mcpServers": {"other": {"command": "x"}}}"#;
    let (text, action) = json(Some(before), "mcpServers", &entry(), json!({})).unwrap();
    assert_eq!(action, Action::Merge);
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["theme"], "dark");
    assert_eq!(v["mcpServers"]["other"]["command"], "x");
    let (again, action) = json(Some(&text), "mcpServers", &entry(), json!({})).unwrap();
    assert_eq!((again, action), (text, Action::Unchanged));
}

#[test]
fn json_with_comments_is_refused_not_rewritten() {
    let err = json(Some("// mine\n{}"), "mcp", &entry(), json!({})).unwrap_err();
    assert!(err.contains("by hand"), "{err}");
}

/// A comment in the TOML stays, and so does every other table.
#[test]
fn toml_keeps_comments_and_other_tables() {
    let before = "# my settings\nmodel = \"o4\"\n\n[mcp_servers.other]\ncommand = \"x\"\n";
    let (text, action) = toml(Some(before), &entry()).unwrap();
    assert_eq!(action, Action::Merge);
    assert!(text.contains("# my settings") && text.contains("[mcp_servers.other]"), "{text}");
    assert!(text.contains("[mcp_servers.godwinmix]"), "{text}");
    assert!(text.contains("args = [\"mcp\"]"), "{text}");
    let (_, action) = toml(Some(&text), &entry()).unwrap();
    assert_eq!(action, Action::Unchanged);
}
