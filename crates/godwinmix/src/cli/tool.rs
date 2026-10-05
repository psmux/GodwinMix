//! `gmx tool`: the MCP tools, one at a time, from a shell.
//!
//! For an agent that has a shell and no MCP, pi for one, and for a person
//! trying a tool by hand. It runs the very tool table `godwinmix mcp` serves
//! (`mcp::Server::handle`), so a skill that says `add_source` means the same
//! thing here as over MCP, and there is no second list of tools to drift.
//!
//! ```text
//!   gmx tool list                          the tools an agent is shown first
//!   gmx tool search_tools '{"query": "lower third"}'
//!   gmx tool add_source '{"name": "news", "uri": "template:breaking-news"}'
//!   gmx tool save_template @design.json
//! ```
//!
//! The answer is printed as the tool gives it. A refusal is printed too and
//! the command exits 1, so a script and an agent both see it failed.

use crate::mcp::Server;
use godwinmix_protocol::scope::Profile;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

pub async fn run(url: &str, token: Option<String>, name: &str, args: Option<&str>) -> Result<()> {
    let server = Server::new(url, token, Profile::Standard);
    if name == "list" {
        let tools = server.tools();
        for t in &tools {
            let first = t["description"].as_str().unwrap_or_default().split(". ").next().unwrap_or_default();
            println!("{:<24} {first}", t["name"].as_str().unwrap_or_default());
        }
        println!("\nThese are the ones shown first. Every other tool is found with:\n  gmx tool search_tools '{{\"query\": \"what you want to do\"}}'");
        return Ok(());
    }
    let arguments = arguments(args)?;
    let request = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": arguments}});
    let reply = server.handle(&request.to_string()).await.context("the tool server gave no answer")?;
    if let Some(e) = reply.get("error") {
        bail!("{}", e["message"].as_str().unwrap_or("the tool call failed"));
    }
    let result = &reply["result"];
    for part in result["content"].as_array().into_iter().flatten() {
        match part["type"].as_str() {
            Some("text") => println!("{}", part["text"].as_str().unwrap_or_default()),
            Some("image") => println!("{}", picture(name, part)),
            _ => println!("{part}"),
        }
    }
    if result["isError"].as_bool() == Some(true) {
        std::process::exit(1);
    }
    Ok(())
}

/// A picture a tool answered with, written to a file an agent with a shell
/// can open and look at, since a terminal cannot show it. Answers the line
/// to print.
fn picture(tool: &str, part: &Value) -> String {
    use base64::Engine;
    let ext = if part["mimeType"].as_str().unwrap_or_default().contains("png") { "png" } else { "jpg" };
    let bytes = base64::engine::general_purpose::STANDARD.decode(part["data"].as_str().unwrap_or_default()).unwrap_or_default();
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let path = std::env::temp_dir().join(format!("gmx-{tool}-{stamp}.{ext}"));
    match std::fs::write(&path, &bytes) {
        Ok(()) => format!("picture: {} ({} bytes). Open that file to look at it.", path.display(), bytes.len()),
        Err(e) => format!("(a picture of {} bytes, which could not be written to {}: {e})", bytes.len(), path.display()),
    }
}

/// The tool's arguments: JSON on the command line, `@file`, or nothing.
fn arguments(args: Option<&str>) -> Result<Value> {
    let Some(text) = args.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(json!({})) };
    let text = match text.strip_prefix('@') {
        Some(path) => std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?,
        None => text.to_string(),
    };
    let v: Value = serde_json::from_str(&text).context("the arguments are not JSON; give an object such as '{\"id\": \"cam1\"}'")?;
    if !v.is_object() {
        bail!("the arguments are a JSON object, such as '{{\"id\": \"cam1\"}}', not {v}");
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_an_object_inline_or_from_a_file() {
        assert_eq!(arguments(None).unwrap(), json!({}));
        assert_eq!(arguments(Some(r#"{"id": "cam1"}"#)).unwrap(), json!({"id": "cam1"}));
        assert!(arguments(Some("[1]")).is_err());
        assert!(arguments(Some("not json")).is_err());
        let path = std::env::temp_dir().join(format!("gmx-tool-{}.json", std::process::id()));
        std::fs::write(&path, r#"{"name": "x"}"#).unwrap();
        assert_eq!(arguments(Some(&format!("@{}", path.display()))).unwrap(), json!({"name": "x"}));
        let _ = std::fs::remove_file(&path);
    }
}
