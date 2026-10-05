//! Setting an AI agent tool up to use this mixer, in one step.
//!
//! `agent.setup` (the Set up button in Help > Connect an AI agent) and
//! `gmx agent setup <tool>` both come here. A setup is two kinds of file: the
//! tool's MCP config, with one entry named `godwinmix` that starts this
//! mixer's own executable as `godwinmix mcp`, and the three skills. Planning
//! reads and writes nothing but answers what it would write, which is the dry
//! run the dialog shows before asking. Applying writes it, keeping a copy of
//! any file it changes beside the original.
//!
//! The entry carries no address and no token. `godwinmix mcp` finds the
//! desktop app's mixer by itself (`crate::address`); a mixer that is not the
//! desktop app's is named in `env`, which the caller passes on purpose.

pub mod detect;
mod dirs;
mod files;
pub mod merge;
pub mod targets;

pub use files::{apply, path_note, with_note};
use merge::Action;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
pub use targets::{AgentTool, Dirs, SetupScope};

/// `agent.setup`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetupRequest {
    /// claude, opencode, pi, codex, gemini, cursor, vscode or other.
    pub tool: AgentTool,
    /// `user` (the default) writes into the home folder, `project` into `dir`.
    #[serde(default)]
    pub scope: SetupScope,
    /// The project folder, for `scope: project`. An absolute path.
    #[serde(default)]
    pub dir: Option<String>,
    /// Environment for `godwinmix mcp`, such as GODWINMIX_URL for a mixer
    /// that is not the desktop app's. Written into the tool's config as given.
    #[serde(default)]
    pub env: Map<String, Value>,
    /// Set by the dispatcher's `dry_run`, never sent.
    #[serde(skip)]
    pub dry_run: bool,
}

/// One file a setup writes.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct FileWrite {
    pub path: String,
    pub action: Action,
    /// In words: "the godwinmix MCP server", "the godwinmix-design skill".
    pub what: String,
    /// Where the file was copied before it was changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<String>,
    #[serde(skip)]
    pub text: String,
    /// A skill file is ours to replace; a tool's config is copied aside first.
    #[serde(skip)]
    pub ours: bool,
}

/// What a setup did, or would do.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct Setup {
    pub tool: AgentTool,
    pub name: &'static str,
    pub scope: SetupScope,
    pub writes: Vec<FileWrite>,
    /// How to start the tool afterwards.
    pub start: String,
    /// A first thing to ask it.
    pub prompt: String,
    /// The MCP entry, for a client nothing is written for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    pub applied: bool,
}

pub const FIRST_PROMPT: &str =
    "Make a lower third for Ana Silva, Producer, in blue, and put it on air.";

/// Work out every write, reading what is there and changing nothing.
pub fn plan(req: &SetupRequest, exe: &Path, dirs: &Dirs) -> Result<Setup, String> {
    let project = project_dir(req)?;
    let target = req.tool.target(req.scope, dirs, &project);
    let exe_text = exe.display().to_string();
    let entry = req.tool.entry(&exe_text, &req.env);
    let mut writes = Vec::new();
    if let Some((path, format)) = &target.mcp {
        let mut mcp = files::mcp_write(path, *format, &entry, req.tool.seed())?;
        if let Some(rules) = rules_file(req.tool, req.scope, dirs, &project) {
            writes.push(files::rules_write(&rules, &mut mcp)?);
        }
        writes.insert(0, mcp);
    }
    let note = files::path_note(exe);
    if let Some(dir) = &target.skills {
        writes.extend(files::skill_writes(dir, note.as_deref()));
    }
    let mut notes = Vec::new();
    if req.tool == AgentTool::Claude && req.scope == SetupScope::Project {
        notes.push("Claude Code asks once whether to use the project's MCP server. Say yes.".into());
    }
    if req.tool == AgentTool::Claude && req.scope == SetupScope::User {
        notes.push("Quit Claude Code before setting it up, or it may write its own copy of ~/.claude.json over this one.".into());
    }
    Ok(Setup {
        tool: req.tool,
        name: req.tool.name(),
        scope: req.scope,
        writes,
        start: start(req.tool, req.scope, &project),
        prompt: FIRST_PROMPT.into(),
        entry: (req.tool == AgentTool::Other).then(|| serde_json::json!({ "mcpServers": { merge::NAME: entry } })),
        notes,
        applied: false,
    })
}

/// A few lines read into every opencode session, named in its config's
/// `instructions`. A free model in opencode asked "what is on air right now?"
/// checked the clock and asked which country; it never thought of the mixer.
fn rules_file(tool: AgentTool, scope: SetupScope, dirs: &Dirs, project: &Path) -> Option<PathBuf> {
    match (tool, scope) {
        (AgentTool::Opencode, SetupScope::User) => Some(dirs.home.join(".config").join("opencode").join("godwinmix.md")),
        (AgentTool::Opencode, SetupScope::Project) => Some(project.join(".opencode").join("godwinmix.md")),
        _ => None,
    }
}

fn project_dir(req: &SetupRequest) -> Result<PathBuf, String> {
    match (req.scope, req.dir.as_deref().map(str::trim)) {
        (SetupScope::User, _) => Ok(PathBuf::new()),
        (SetupScope::Project, Some(dir)) if Path::new(dir).is_absolute() => Ok(PathBuf::from(dir)),
        (SetupScope::Project, Some(dir)) if !dir.is_empty() => {
            Err(format!("`dir` is {dir:?}, which is not an absolute path. Send the project folder's full path."))
        }
        (SetupScope::Project, _) => {
            Err("scope project needs `dir`, the project folder's full path. Or use scope user.".into())
        }
    }
}

fn start(tool: AgentTool, scope: SetupScope, project: &Path) -> String {
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
