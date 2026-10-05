//! Where each agent tool keeps its MCP servers and its skills, per scope and
//! per operating system, and the entry it wants for this mixer.
//!
//! Every path here was checked against the tool's own documentation:
//!
//! | Tool | MCP servers (user, project) | Skills (user, project) |
//! |---|---|---|
//! | Claude Code | `~/.claude.json`, `.mcp.json` | `~/.claude/skills`, `.claude/skills` |
//! | opencode | `~/.config/opencode/opencode.json`, `opencode.json` | `~/.config/opencode/skills`, `.opencode/skills` |
//! | pi | none, by design | `~/.agents/skills`, `.agents/skills` |
//! | Codex | `~/.codex/config.toml`, `.codex/config.toml` | `~/.codex/skills`, `.codex/skills` |
//! | Gemini CLI | `~/.gemini/settings.json`, `.gemini/settings.json` | `~/.gemini/skills`, `.gemini/skills` |
//! | Cursor | `~/.cursor/mcp.json`, `.cursor/mcp.json` | `~/.cursor/skills`, `.cursor/skills` |
//! | VS Code | the user `mcp.json`, `.vscode/mcp.json` | `~/.copilot/skills`, `.github/skills` |
//!
//! opencode uses `~/.config` on every system, Windows included. VS Code keeps
//! its user folder where each system keeps application data.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub use super::dirs::Dirs;

/// The agent tools this mixer knows how to set up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum AgentTool {
    Claude,
    Opencode,
    Pi,
    Codex,
    Gemini,
    Cursor,
    Vscode,
    /// Any other MCP client: nothing is written, the entry is shown to paste.
    Other,
}

pub const ALL: [AgentTool; 8] = [
    AgentTool::Claude,
    AgentTool::Opencode,
    AgentTool::Pi,
    AgentTool::Codex,
    AgentTool::Gemini,
    AgentTool::Cursor,
    AgentTool::Vscode,
    AgentTool::Other,
];

/// For the user, in their home folder, or for one project folder.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum SetupScope {
    #[default]
    User,
    Project,
}

/// How a tool's MCP file is laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// JSON, with the servers in an object under this key.
    Json(&'static str),
    /// Codex's TOML, `[mcp_servers.<name>]`.
    Toml,
}

/// What one tool needs written.
#[derive(Debug, Clone)]
pub struct Target {
    pub mcp: Option<(PathBuf, Format)>,
    pub skills: Option<PathBuf>,
}

impl AgentTool {
    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Opencode => "opencode",
            Self::Pi => "pi",
            Self::Codex => "Codex",
            Self::Gemini => "Gemini CLI",
            Self::Cursor => "Cursor",
            Self::Vscode => "VS Code",
            Self::Other => "Any MCP client",
        }
    }

    /// The command that starts it, and the name it has on PATH.
    pub fn command(self) -> Option<&'static str> {
        match self {
            Self::Claude => Some("claude"),
            Self::Opencode => Some("opencode"),
            Self::Pi => Some("pi"),
            Self::Codex => Some("codex"),
            Self::Gemini => Some("gemini"),
            Self::Cursor => Some("cursor"),
            Self::Vscode => Some("code"),
            Self::Other => None,
        }
    }

    pub fn target(self, scope: SetupScope, dirs: &Dirs, project: &Path) -> Target {
        let user = scope == SetupScope::User;
        let root = if user { dirs.home.as_path() } else { project };
        let at = |parts: &[&str]| parts.iter().fold(root.to_path_buf(), |p, s| p.join(s));
        let json = |path: PathBuf, key| Some((path, Format::Json(key)));
        match self {
            Self::Claude => Target {
                mcp: json(if user { at(&[".claude.json"]) } else { at(&[".mcp.json"]) }, "mcpServers"),
                skills: Some(at(&[".claude", "skills"])),
            },
            Self::Opencode => Target {
                mcp: json(
                    if user { at(&[".config", "opencode", "opencode.json"]) } else { at(&["opencode.json"]) },
                    "mcp",
                ),
                skills: Some(if user { at(&[".config", "opencode", "skills"]) } else { at(&[".opencode", "skills"]) }),
            },
            Self::Pi => Target { mcp: None, skills: Some(at(&[".agents", "skills"])) },
            Self::Codex => Target {
                mcp: Some((at(&[".codex", "config.toml"]), Format::Toml)),
                skills: Some(at(&[".codex", "skills"])),
            },
            Self::Gemini => Target {
                mcp: json(at(&[".gemini", "settings.json"]), "mcpServers"),
                skills: Some(at(&[".gemini", "skills"])),
            },
            Self::Cursor => Target {
                mcp: json(at(&[".cursor", "mcp.json"]), "mcpServers"),
                skills: Some(at(&[".cursor", "skills"])),
            },
            Self::Vscode => Target {
                mcp: json(
                    if user { dirs.app_config.join("Code").join("User").join("mcp.json") } else { at(&[".vscode", "mcp.json"]) },
                    "servers",
                ),
                skills: Some(if user { at(&[".copilot", "skills"]) } else { at(&[".github", "skills"]) }),
            },
            Self::Other => Target { mcp: None, skills: None },
        }
    }

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
