//! Which agent tools are on this machine, so the dialog shows them first.
//!
//! A tool counts as installed when its command is on PATH or its config
//! folder exists. Either is a guess: a tool installed somewhere odd and never
//! run has neither, and is still listed, only further down.

use super::targets::{AgentTool, Dirs, ALL};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
pub struct Detected {
    pub tool: AgentTool,
    pub name: &'static str,
    pub installed: bool,
    /// What was found: the command's path, or the config folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub found: Option<String>,
}

/// Every tool, the installed ones first, each group in the usual order.
pub fn detect(dirs: &Dirs, path_var: Option<&std::ffi::OsStr>) -> Vec<Detected> {
    let mut all: Vec<Detected> = ALL
        .iter()
        .map(|&tool| {
            let found = tool
                .command()
                .and_then(|c| on_path(c, path_var))
                .or_else(|| folder(tool, dirs).filter(|p| p.is_dir()));
            Detected { tool, name: tool.name(), installed: found.is_some(), found: found.map(|p| p.display().to_string()) }
        })
        .collect();
    all.sort_by_key(|d| !d.installed);
    all
}

/// The folder a tool makes the first time it runs.
fn folder(tool: AgentTool, dirs: &Dirs) -> Option<PathBuf> {
    let home = &dirs.home;
    Some(match tool {
        AgentTool::Claude => home.join(".claude"),
        AgentTool::Opencode => home.join(".config").join("opencode"),
        AgentTool::Pi => home.join(".pi"),
        AgentTool::Codex => home.join(".codex"),
        AgentTool::Gemini => home.join(".gemini"),
        AgentTool::Cursor => home.join(".cursor"),
        AgentTool::Vscode => dirs.app_config.join("Code"),
        AgentTool::Other => return None,
    })
}

/// A command on PATH, the way a shell finds it. On Windows each PATHEXT
/// ending is tried too, since `claude` is `claude.exe` and `pi` is `pi.cmd`.
pub fn on_path(command: &str, path_var: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    let endings: Vec<String> = if cfg!(windows) {
        let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.PS1".into());
        std::iter::once(String::new()).chain(pathext.split(';').map(|e| e.to_lowercase())).collect()
    } else {
        vec![String::new()]
    };
    std::env::split_paths(path_var?).find_map(|dir| {
        endings.iter().map(|e| dir.join(format!("{command}{e}"))).find(|p| is_program(p))
    })
}

fn is_program(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gmx-detect-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A command on PATH and a config folder both count, and the installed
    /// tools come first.
    #[test]
    fn a_command_on_path_or_a_config_folder_is_installed() {
        let home = scratch("home");
        let bin = scratch("bin");
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        let exe = if cfg!(windows) { "opencode.exe" } else { "opencode" };
        std::fs::write(bin.join(exe), b"").unwrap();
        let dirs = Dirs { home: home.clone(), app_config: home.join("app") };
        let path = std::env::join_paths([&bin]).unwrap();
        let found = detect(&dirs, Some(&path));
        let installed: Vec<AgentTool> = found.iter().filter(|d| d.installed).map(|d| d.tool).collect();
        assert_eq!(installed, vec![AgentTool::Opencode, AgentTool::Codex]);
        assert_eq!(found[0].tool, AgentTool::Opencode, "installed first");
        assert!(!found.iter().any(|d| d.tool == AgentTool::Claude && d.installed));
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&bin);
    }
}
