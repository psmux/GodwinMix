//! The files a setup writes: what goes in each, and writing them safely.

use super::merge::{self, Action};
use super::targets::Format;
use super::{FileWrite, Setup};
use serde_json::Value;
use std::path::Path;

/// The MCP config, with the `godwinmix` entry merged in.
pub fn mcp_write(path: &Path, format: Format, entry: &Value, seed: Value) -> Result<FileWrite, String> {
    let existing = std::fs::read_to_string(path).ok();
    let merged = match format {
        Format::Json(key) => merge::json(existing.as_deref(), key, entry, seed),
        Format::Toml => merge::toml(existing.as_deref(), entry),
    };
    let (text, action) = merged.map_err(|why| {
        format!("{} cannot be changed safely: {why}:\n{}", path.display(), serde_json::to_string_pretty(entry).unwrap_or_default())
    })?;
    Ok(FileWrite { path: path.display().to_string(), action, what: "the godwinmix MCP server".into(), backup: None, text, ours: false })
}

/// The rules file, and its path added to the config's `instructions`.
pub fn rules_write(path: &Path, config: &mut FileWrite) -> Result<FileWrite, String> {
    let item = path.display().to_string();
    let (text, added) = merge::json_list_add(&config.text, "instructions", &item)
        .map_err(|why| format!("{} cannot be changed safely: {why}", config.path))?;
    if added {
        config.text = text;
        if config.action == Action::Unchanged {
            config.action = Action::Merge;
        }
    }
    let body = include_str!("rules.md");
    let action = match std::fs::read_to_string(path) {
        Err(_) => Action::Create,
        Ok(old) if old == body => Action::Unchanged,
        Ok(_) => Action::Update,
    };
    Ok(FileWrite { path: item, action, what: "what GodwinMix is, read every session".into(), backup: None, text: body.into(), ours: true })
}

/// One `SKILL.md` per skill, with the note about PATH when there is one.
pub fn skill_writes(dir: &Path, note: Option<&str>) -> Vec<FileWrite> {
    crate::cli::skill::skills()
        .into_iter()
        .map(|(name, text)| {
            let text = with_note(&text, note);
            let path = dir.join(name).join("SKILL.md");
            let action = match std::fs::read_to_string(&path) {
                Err(_) => Action::Create,
                Ok(old) if old == text => Action::Unchanged,
                Ok(_) => Action::Update,
            };
            FileWrite { path: path.display().to_string(), action, what: format!("the {name} skill"), backup: None, text, ours: true }
        })
        .collect()
}

/// A line for the skills when `godwinmix` on PATH is not this executable.
///
/// The skills say `godwinmix tool ...`, and an installed app is on nobody's
/// PATH: pi ran `godwinmix` in its shell and got "command not found". With
/// this line it runs the app's own executable by its full path.
pub fn path_note(exe: &Path) -> Option<String> {
    let found = super::detect::on_path("godwinmix", std::env::var_os("PATH").as_deref());
    let same = found.as_deref().and_then(|p| p.canonicalize().ok()) == exe.canonicalize().ok();
    if found.is_some() && same {
        return None;
    }
    let e = exe.display();
    // Bash first: pi, and Claude Code's Bash tool on Windows, run Git Bash.
    // A free model shown the PowerShell form first sent it to bash and lost a
    // turn to the error.
    Some(format!(
        "> On this machine `godwinmix` is not on PATH. Wherever this skill says \
         `godwinmix`, run \"{e}\" instead, in double quotes. Only PowerShell needs \
         an & in front of it."
    ))
}

/// The note goes straight after the frontmatter, where an agent reads first.
pub fn with_note(skill: &str, note: Option<&str>) -> String {
    let Some(note) = note else { return skill.to_string() };
    let Some(rest) = skill.strip_prefix("---\n") else { return format!("{note}\n\n{skill}") };
    match rest.find("\n---\n") {
        Some(end) => {
            let (front, body) = rest.split_at(end + 5);
            format!("---\n{front}\n{note}\n{body}")
        }
        None => format!("{skill}\n\n{note}\n"),
    }
}

/// Write every planned file that changes, copying an existing file aside
/// first. Nothing is written for an `Unchanged` file.
pub fn apply(setup: &mut Setup) -> Result<(), String> {
    for w in setup.writes.iter_mut().filter(|w| w.action != Action::Unchanged) {
        let path = Path::new(&w.path);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
        }
        if path.is_file() && !w.ours {
            let backup = backup_path(path);
            std::fs::copy(path, &backup).map_err(|e| format!("cannot copy {} aside: {e}", path.display()))?;
            w.backup = Some(backup.display().to_string());
        }
        std::fs::write(path, &w.text).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    setup.applied = true;
    Ok(())
}

/// `<file>.before-godwinmix`, or with a number when that is taken, so the
/// first original is never overwritten by a later copy.
fn backup_path(path: &Path) -> std::path::PathBuf {
    let base = format!("{}.before-godwinmix", path.display());
    (0..)
        .map(|n| if n == 0 { base.clone() } else { format!("{base}.{n}") })
        .map(std::path::PathBuf::from)
        .find(|p| !p.exists())
        .expect("an unused name")
}
