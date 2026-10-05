//! What goes into each tool's config, and what to say once it is written.

use super::targets::{AgentTool, Dirs, SetupScope};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

impl AgentTool {
    /// This tool's entry for the mixer at `exe`, with `env` when the mixer
    /// cannot be found without it.
    pub fn entry(self, exe: &str, env: &Map<String, Value>) -> Value {
        let env_value = Value::Object(env.clone());
        let mut entry = match self {
            Self::Opencode => json!({ "type": "local", "command": [exe, "mcp"], "enabled": true }),
            Self::Claude | Self::Vscode => json!({ "type": "stdio", "command": exe, "args": ["mcp"] }),
            _ => json!({ "command": exe, "args": ["mcp"] }),
        };
        if !env.is_empty() {
            let key = if self == Self::Opencode { "environment" } else { "env" };
            entry[key] = env_value;
        }
        entry
    }

    /// What a new file starts as, before the entry goes in.
    pub fn seed(self) -> Value {
        match self {
            Self::Opencode => json!({ "$schema": "https://opencode.ai/config.json" }),
            _ => json!({}),
        }
    }
}

/// A few lines read into every opencode session, named in its config's
/// `instructions`. A free model in opencode asked "what is on air right now?"
/// checked the clock and asked which country; it never thought of the mixer.
pub fn rules_file(tool: AgentTool, scope: SetupScope, dirs: &Dirs, project: &Path) -> Option<PathBuf> {
    match (tool, scope) {
        (AgentTool::Opencode, SetupScope::User) => Some(dirs.home.join(".config").join("opencode").join("godwinmix.md")),
        (AgentTool::Opencode, SetupScope::Project) => Some(project.join(".opencode").join("godwinmix.md")),
        _ => None,
    }
}

/// How to start the tool once it is set up.
pub fn start(tool: AgentTool, scope: SetupScope, project: &Path) -> String {
    let there = match scope {
        SetupScope::Project => format!("In a terminal in {}, run", project.display()),
        SetupScope::User => "In a terminal, run".into(),
    };
    match tool {
        AgentTool::Cursor => "Restart Cursor and check godwinmix is switched on under Settings > MCP.".into(),
        AgentTool::Vscode => "Reload VS Code and open Copilot Chat in Agent mode; godwinmix is in its tools.".into(),
        AgentTool::Other => "Paste the entry into your client's MCP settings and restart it.".into(),
        t => format!("{there} `{}`.", t.command().unwrap_or_default()),
    }
}
