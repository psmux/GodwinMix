//! `gmx agent setup <tool>` and `gmx agent tools`: the Set up button, from a
//! terminal, on this machine.
//!
//! The same plan and the same writes as `agent.setup` (`crate::agents`). It
//! runs here rather than through a mixer because the files belong to this
//! machine's user, and a person setting up an agent may not have a mixer
//! running yet.
//!
//! ```text
//!   gmx agent setup claude                  ~/.claude.json and ~/.claude/skills
//!   gmx agent setup opencode --project .    ./opencode.json and ./.opencode/skills
//!   gmx agent setup codex --dry-run         what it would write, writing nothing
//! ```

use crate::agents::{self, AgentTool, Dirs, SetupRequest, SetupScope};
use anyhow::{bail, Context, Result};
use serde_json::{json, Map};

#[derive(Debug, Clone, clap::Args)]
pub struct SetupArgs {
    /// claude, opencode, pi, codex, gemini, cursor, vscode, or other to print the entry.
    pub tool: AgentTool,
    /// Set it up for one project folder instead of for this user.
    #[arg(long, value_name = "DIR", num_args = 0..=1, default_missing_value = ".")]
    pub project: Option<std::path::PathBuf>,
    /// Print every file it would write, and write nothing.
    #[arg(long)]
    pub dry_run: bool,
    /// A mixer that is not the desktop app's: written into the config as GODWINMIX_URL.
    #[arg(long)]
    pub url: Option<String>,
    /// Its token, written into the config as GODWINMIX_TOKEN. Only with --url.
    #[arg(long, requires = "url")]
    pub token: Option<String>,
}

pub fn run(args: SetupArgs) -> Result<()> {
    let dirs = Dirs::from_env().context("no home folder: set HOME (or USERPROFILE on Windows)")?;
    let exe = std::env::current_exe().context("finding this executable")?;
    let (scope, dir) = match &args.project {
        Some(p) => (SetupScope::Project, Some(std::path::absolute(p)?.display().to_string())),
        None => (SetupScope::User, None),
    };
    let mut env = Map::new();
    if let Some(url) = &args.url {
        env.insert("GODWINMIX_URL".into(), json!(url));
    }
    if let Some(token) = &args.token {
        env.insert("GODWINMIX_TOKEN".into(), json!(token));
    }
    let req = SetupRequest { tool: args.tool, scope, dir, env, dry_run: args.dry_run };
    let mut setup = match agents::plan(&req, &exe, &dirs) {
        Ok(s) => s,
        Err(why) => bail!("{why}"),
    };
    if !args.dry_run {
        if let Err(why) = agents::apply(&mut setup) {
            bail!("{why}");
        }
    }
    print(&setup, args.dry_run);
    if args.url.is_none() && crate::address::desktop().is_none() {
        println!(
            "\nNo GodwinMix app is running on this machine, so the agent will look for a mixer \
             at {} unless you open the app first, or run this again with --url (and --token).",
            crate::DEFAULT_URL
        );
    }
    Ok(())
}

fn print(setup: &agents::Setup, dry_run: bool) {
    let verb = if dry_run { "would write" } else { "wrote" };
    println!("{} ({}):", setup.name, if setup.scope == SetupScope::User { "for this user" } else { "for this project" });
    for w in &setup.writes {
        let action = format!("{:?}", w.action).to_lowercase();
        println!("  {verb:<11} {:<9} {}  ({})", action, w.path, w.what);
        if let Some(b) = &w.backup {
            println!("              the old file is at {b}");
        }
    }
    if let Some(entry) = &setup.entry {
        println!("Paste this into your client's MCP settings:\n{}", serde_json::to_string_pretty(entry).unwrap_or_default());
    }
    for note in &setup.notes {
        println!("Note: {note}");
    }
    if dry_run {
        println!("\nRun it again without --dry-run to write them.");
    } else {
        println!("\nNext: {}\nThen ask: {}", setup.start, setup.prompt);
    }
}

pub fn tools() -> Result<()> {
    let dirs = Dirs::from_env().context("no home folder: set HOME (or USERPROFILE on Windows)")?;
    for d in agents::detect::detect(&dirs, std::env::var_os("PATH").as_deref()) {
        let id = serde_json::to_value(d.tool).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
        let found = d.found.unwrap_or_else(|| "not found".into());
        println!("{id:<9} {:<15} {found}", d.name);
    }
    println!("\nSet one up with: gmx agent setup <tool>");
    Ok(())
}
