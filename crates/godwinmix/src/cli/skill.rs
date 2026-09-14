//! `gmx skill install`: drop the skills where an agent tool will find them.
//!
//! Two skills ship with the mixer. `godwinmix-operate` is for an agent running
//! a show: the state document, the take, the safety rules that will refuse it
//! and what a look costs. `godwinmix-develop` is for one building on it: the
//! manifest, the contract and the test loop.
//!
//! They are read from `skills/` at runtime where a checkout or an install has
//! one, and from a copy compiled into the binary where it does not, so a
//! single downloaded `gmx` still installs them.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// The two skills, compiled in, so a binary on its own still has them.
const OPERATE: &str = include_str!("../../../../skills/godwinmix-operate/SKILL.md");
const DEVELOP: &str = include_str!("../../../../skills/godwinmix-develop/SKILL.md");

/// The names, in the order they are installed.
pub const SKILLS: [&str; 2] = ["godwinmix-operate", "godwinmix-develop"];

#[derive(Debug, Clone, clap::Args)]
pub struct SkillArgs {
    #[command(subcommand)]
    pub cmd: SkillCmd,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum SkillCmd {
    /// Install the GodwinMix skills for an AI coding tool.
    Install {
        /// Which tool's directory to write into.
        #[arg(long = "for", value_name = "TOOL", default_value = "claude")]
        tool: Tool,
        /// Print what would be written and write nothing.
        #[arg(long)]
        print: bool,
        /// Write into this directory instead of the tool's own.
        #[arg(long, value_name = "DIR")]
        dir: Option<PathBuf>,
        /// Install into the project rather than into the home directory.
        #[arg(long)]
        project: bool,
    },
    /// List the skills this binary can install and where they would go.
    List {
        #[arg(long = "for", value_name = "TOOL", default_value = "claude")]
        tool: Tool,
    },
}

/// The agent tools that read a skill directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Tool {
    Claude,
    Codex,
    Gemini,
}

impl Tool {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
        }
    }

    /// Where this tool reads skills from.
    ///
    /// `project` is the checked in form, which a team shares; the other is the
    /// per user one. Both are directories of `<name>/SKILL.md`.
    pub fn directory(self, project: bool) -> Option<PathBuf> {
        let root = if project {
            std::env::current_dir().ok()?
        } else {
            home()?
        };
        Some(match (self, project) {
            (Self::Claude, true) => root.join(".claude/skills"),
            (Self::Claude, false) => root.join(".claude/skills"),
            (Self::Codex, true) => root.join(".codex/skills"),
            (Self::Codex, false) => root.join(".codex/skills"),
            (Self::Gemini, true) => root.join(".gemini/skills"),
            (Self::Gemini, false) => root.join(".gemini/skills"),
        })
    }
}

fn home() -> Option<PathBuf> {
    // No `dirs` crate for two environment variables. `USERPROFILE` is what
    // Windows sets; `HOME` is everywhere else, and is set on Windows under a
    // unix shell too.
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

/// One skill's name and body, from the filesystem where there is one.
pub fn skills() -> Vec<(&'static str, String)> {
    let from_disk = source_dir();
    SKILLS
        .iter()
        .map(|name| {
            let text = from_disk
                .as_ref()
                .and_then(|dir| std::fs::read_to_string(dir.join(name).join("SKILL.md")).ok())
                .unwrap_or_else(|| embedded(name).to_string());
            (*name, text)
        })
        .collect()
}

fn embedded(name: &str) -> &'static str {
    match name {
        "godwinmix-develop" => DEVELOP,
        _ => OPERATE,
    }
}

/// A `skills/` directory beside the binary, above it, or in the working
/// directory. An installed package puts one in `share/godwinmix/skills`.
fn source_dir() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("skills"));
            candidates.push(dir.join("../share/godwinmix/skills"));
            // A cargo target directory: target/debug/gmx.
            candidates.push(dir.join("../../skills"));
            candidates.push(dir.join("../../../skills"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("skills"));
    }
    candidates
        .into_iter()
        .find(|dir| SKILLS.iter().all(|s| dir.join(s).join("SKILL.md").is_file()))
}

pub fn run(cmd: SkillCmd) -> Result<()> {
    match cmd {
        SkillCmd::List { tool } => {
            let home = tool.directory(false);
            let project = tool.directory(true);
            println!("skills this build can install:");
            for (name, text) in skills() {
                println!("  {name:<20} {} bytes", text.len());
            }
            println!(
                "\nfor {}:\n  home    {}\n  project {}",
                tool.as_str(),
                show(home.as_deref()),
                show(project.as_deref())
            );
            Ok(())
        }
        SkillCmd::Install { tool, print, dir, project } => {
            let target = match dir {
                Some(dir) => dir,
                None => tool.directory(project).with_context(|| {
                    format!(
                        "no home directory to install into for {}. Pass --dir to say where, \
                         or set HOME.",
                        tool.as_str()
                    )
                })?,
            };
            install(&target, print, tool)
        }
    }
}

fn show(path: Option<&Path>) -> String {
    path.map(|p| p.display().to_string()).unwrap_or_else(|| "(unknown)".into())
}

fn install(target: &Path, print: bool, tool: Tool) -> Result<()> {
    let skills = skills();
    if print {
        println!("would write, for {}:", tool.as_str());
        for (name, text) in &skills {
            let path = target.join(name).join("SKILL.md");
            let what = if path.exists() { "replace" } else { "create" };
            println!("  {what:<8} {} ({} bytes)", path.display(), text.len());
        }
        println!("\nRun again without --print to write them.");
        return Ok(());
    }
    for (name, text) in &skills {
        let dir = target.join(name);
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("creating {}", dir.display()))?;
        let path = dir.join("SKILL.md");
        std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
        println!("wrote {}", path.display());
    }
    println!(
        "\n{} now reads {} and {}. Start a session in a directory with a mixer to hand, or \
         point it at one with GODWINMIX_URL.",
        tool.as_str(),
        SKILLS[0],
        SKILLS[1]
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both skills exist, carry the frontmatter a tool reads, and say what
    /// they are for in a sentence that names when to use them.
    #[test]
    fn every_skill_has_the_frontmatter_a_tool_needs() {
        for (name, text) in skills() {
            assert!(text.starts_with("---\n"), "{name} has no frontmatter");
            let front = text.split("---").nth(1).expect("frontmatter");
            assert!(front.contains(&format!("name: {name}")), "{name}: {front}");
            assert!(front.contains("description:"), "{name} has no description");
            // A description that does not say when to use the skill is a
            // description nothing will ever match against.
            let description = front
                .lines()
                .find(|l| l.starts_with("description:"))
                .expect("description");
            assert!(
                description.contains("Use when") || description.contains("Use it when"),
                "{name}: the description has to say when to use it: {description}"
            );
            assert!(text.len() > 500, "{name} is too short to be a skill");
        }
    }

    /// The operate skill says the things an agent will get wrong without it.
    #[test]
    fn the_operate_skill_names_the_rules_that_will_refuse_an_agent() {
        let text = skills()
            .into_iter()
            .find(|(n, _)| *n == "godwinmix-operate")
            .expect("the operate skill")
            .1;
        for expected in [
            "agent_state",
            "-32003",
            "retry_after_ms",
            "BT.1702-3",
            "revert",
            "idempotency_key",
            "indeterminate",
            "notifications/gmx/agent.state",
            "rehearsal",
        ] {
            assert!(text.contains(expected), "the operate skill never mentions {expected}");
        }
    }

    #[test]
    fn a_tool_directory_is_named_for_every_tool() {
        // With HOME set, which it is everywhere a person runs this.
        for tool in [Tool::Claude, Tool::Codex, Tool::Gemini] {
            let dir = tool.directory(false).expect("a home directory");
            let text = dir.display().to_string();
            assert!(text.contains("skills"), "{text}");
            assert!(text.contains(tool.as_str()), "{text}");
        }
    }

    /// `--print` writes nothing, which is the whole point of it.
    #[test]
    fn print_writes_nothing() {
        let dir = std::env::temp_dir().join(format!("gmx-skill-print-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        install(&dir, true, Tool::Claude).expect("printing cannot fail");
        assert!(!dir.exists(), "--print created {}", dir.display());
    }

    #[test]
    fn installing_writes_one_file_per_skill_and_replaces_an_older_one() {
        let dir = std::env::temp_dir().join(format!("gmx-skill-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        install(&dir, false, Tool::Codex).expect("install");
        for name in SKILLS {
            let path = dir.join(name).join("SKILL.md");
            assert!(path.is_file(), "{} was not written", path.display());
        }
        // A second run replaces rather than failing on the existing file.
        std::fs::write(dir.join(SKILLS[0]).join("SKILL.md"), "stale").unwrap();
        install(&dir, false, Tool::Codex).expect("install again");
        let text = std::fs::read_to_string(dir.join(SKILLS[0]).join("SKILL.md")).unwrap();
        assert_ne!(text, "stale");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
